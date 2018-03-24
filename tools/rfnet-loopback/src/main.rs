extern crate rfnet_core;
extern crate hyper;
extern crate tokio_core;
extern crate futures;
extern crate simple_logger;

use rfnet_core::{Link, LinkConfig, RetryConfig, HttpProvider};
use rfnet_core::framed::{KISSFramed, LoopbackIo};

use std::net;
use std::sync::mpsc;
use std::thread;
use std::io::{Read, Write};

enum Event {
    Data([u8; 256], usize),
    ClientConnected(usize, net::TcpStream),
    ClientDisconnected(usize),
    Exit
}

fn main() {
    simple_logger::init().unwrap();

    let config = LinkConfig {
        link_width: 32,
        fec: true,
        retry_enabled: true,
        retry: RetryConfig::default(1200),
        broadcast_rate: Some(5)
    };
    let mut link = Link::new("CALLSIGN", config);

    let event_loop = tokio_core::reactor::Core::new().unwrap();

    let listen = net::TcpListener::bind("127.0.0.1:8001").unwrap();
    let (send, recv) = mpsc::channel();

    println!("Listening in port 8001");

    {
        let listen_clone = listen.try_clone().expect("Failed to clone socket");
        let listen_send = send.clone();
        let mut last_client_id = 0;

        thread::spawn(move || {
            for stream in listen_clone.incoming() {
                match stream {
                    Ok(stream) => {
                        last_client_id += 1;
                        listen_send.send(Event::ClientConnected(last_client_id, stream)).unwrap();
                    },
                    Err(e) => {
                        println!("Error on listen socket {:?}", e);
                        listen_send.send(Event::Exit).unwrap();

                        return
                    }
                }
            }
        });
    }

    let mut clients = vec!();
    loop { 
        match recv.recv_timeout(::std::time::Duration::from_millis(100)) {
            Ok(event) => match event {
                Event::ClientConnected(id, socket) => {
                    let mut client_socket = socket.try_clone().expect("Unable to clone socket");
                    let mut client_send = send.clone();

                    clients.push((id, socket));

                    thread::spawn(move || {
                        println!("Client {} connected", id);

                        let mut buffer = [0; 256];
                        while let Ok(read) = client_socket.read(&mut buffer) {
                            client_send.send(Event::Data(buffer, read)).unwrap();
                        }

                        println!("Client {} exited", id);
                        client_send.send(Event::ClientDisconnected(id)).unwrap();
                    });
                },
                Event::ClientDisconnected(id) => {
                    clients.retain(|&(vid,_)| vid != id);
                },
                Event::Data(mut data, size) => {
                    struct Http {
                        client: hyper::Client<hyper::client::HttpConnector, hyper::Body>
                    }

                    impl HttpProvider for Http {
                        fn request(&mut self, request: hyper::Request) -> Result<hyper::Response, hyper::Error> {
                            use futures::Future;
                            self.client.request(request).wait()
                        }
                    }

                    let mut http = Http {
                        client: hyper::Client::new(&event_loop.handle())
                    };

                    let mut output = KISSFramed::new(LoopbackIo::new(), 0);
                    link.recv_data(&mut data[..size], &mut output, &mut http).unwrap();

                    if output.get_tnc().buffer().len() > 0 {
                        for &mut (_id, ref mut client) in &mut clients {
                            client.write_all(&output.get_tnc().buffer()[..]).unwrap();
                        }
                    }
                },
                Event::Exit => return
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let mut output = KISSFramed::new(LoopbackIo::new(), 0);
                link.elapsed(100, &mut output).unwrap();

                if output.get_tnc().buffer().len() > 0 {
                    for &mut (_id, ref mut client) in &mut clients {
                        client.write_all(&output.get_tnc().buffer()[..]).unwrap();
                    }
                }
            },
            Err(mpsc::RecvTimeoutError::Disconnected) => return
        }
    }
}