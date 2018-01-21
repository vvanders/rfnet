extern crate iron;
extern crate router;
extern crate staticfile;
#[macro_use]
extern crate log;
extern crate websocket;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;

pub mod proto;

use iron::prelude::*;
use iron::error::HttpResult;
use router::Router;
use staticfile::Static;

use std::thread;
use std::sync::mpsc;
use std::net::TcpStream;

type ClientID = usize;

pub struct WebInterface {
    _iron: iron::Listening,
    _ws_thread: thread::JoinHandle<()>,
    ws_send: mpsc::Sender<ClientEvent>
}

pub struct WebsocketMessage {
    pub id: ClientID,
    pub msg: String
}

enum ClientEvent {
    Connected(ClientID, websocket::sender::Writer<TcpStream>),
    SendMsg(ClientID, String),
    Broadcast(String),
    Disconnected(ClientID)
}

pub fn new<F,T>(http_port: u16, ws_port: u16, out_msg: mpsc::Sender<T>, map_msg: F) -> HttpResult<WebInterface> 
        where F: Fn(WebsocketMessage) -> T + Copy + Send + Sync + 'static, T: Send + 'static {
    let mut router = Router::new();

    router.get("/", Static::new("static-web"), "index");
    router.get("/*", Static::new("static-web"), "files");
    let iron = Iron::new(router).http(format!("localhost:{}", http_port))?;

    let (ws_event_tx, ws_event_rx) = mpsc::channel();

    let ws_server = websocket::sync::Server::bind(format!("localhost:{}", ws_port))?;

    let listen_ws_event_tx = ws_event_tx.clone();
    let ws_thread = thread::spawn(move || {
        thread::spawn(move || {
            let mut next_client_id = 0;

            for request in ws_server.filter_map(Result::ok) {
                let client = request.accept().map_err(|_| ())
                    .and_then(|client| client.split().map_err(|_| ()));

                if let Ok((mut reader, writer)) = client {
                    let client_id = next_client_id;
                    next_client_id += 1;

                    let client_ws_event_tx = listen_ws_event_tx.clone();
                    let client_ws_recv_tx = out_msg.clone();

                    thread::spawn(move || {
                        client_ws_event_tx.send(ClientEvent::Connected(client_id, writer)).unwrap();
                        for message in reader.incoming_messages() {
                            match message {
                                Ok(websocket::OwnedMessage::Text(m)) => {
                                    client_ws_recv_tx.send(map_msg(WebsocketMessage {
                                            id: client_id,
                                            msg: m
                                        })).unwrap();
                                },
                                Ok(websocket::OwnedMessage::Close(_)) => {
                                    client_ws_event_tx.send(ClientEvent::Disconnected(client_id)).unwrap();
                                }
                                Ok(m) => debug!("Unknown message type {:?} on socket {}", m, client_id),
                                Err(e) => {
                                    debug!("Error on websocket {} {:?}, disconnecting", client_id, e);
                                    client_ws_event_tx.send(ClientEvent::Disconnected(client_id)).unwrap();

                                    break
                                }
                            }
                        }
                    });
                }
            }
        });

        let mut clients = vec!();
        loop {
            let event = match ws_event_rx.recv() {
                Ok(e) => e,
                Err(e) => {
                    error!("Failed to recv websocket event {:?}", e);
                    break
                }
            };

            match event {
                ClientEvent::Connected(id, writer) => {
                    info!("Client {} connected", id);
                    clients.push((id, writer));
                },
                ClientEvent::SendMsg(id, msg) => {
                    if let Some(idx) = clients.iter().position(|&(tid,_)| tid == id) {
                        if let Err(e) = clients[idx].1.send_message(&websocket::OwnedMessage::Text(msg)) {
                            info!("Failed to send on socket {} {:?}, closing", id, e);
                            clients[idx].1.shutdown().unwrap_or(());
                        }
                    }
                },
                ClientEvent::Broadcast(msg) => {
                    let msg = websocket::OwnedMessage::Text(msg);
                    for &mut (id, ref mut client) in &mut clients {
                        if let Err(e) = client.send_message(&msg) {
                            info!("Failed to send on socket {} {:?}, closing", id, e);
                            client.shutdown().unwrap_or(());
                        }
                    }
                },
                ClientEvent::Disconnected(id) => {
                    info!("Client {} disconnected", id);
                    clients.retain(|&(tid,_)| tid != id);
                }
            }
        }
    });

    Ok(WebInterface {
        _iron: iron,
        _ws_thread: ws_thread,
        ws_send: ws_event_tx
    })
}

impl WebInterface {
    pub fn close(&mut self) -> HttpResult<()> {
        self._iron.close()
    }

    pub fn broadcast_json(&mut self, json: String) -> Result<(), ()> {
        self.ws_send.send(ClientEvent::Broadcast(json)).map_err(|_| ())
    }

    pub fn send_json(&mut self, id: ClientID, json: String) -> Result<(), ()> {
        self.ws_send.send(ClientEvent::SendMsg(id, json)).map_err(|_| ())
    }

    pub fn broadcast(&mut self, msg: proto::Message) -> Result<(), ()> {
        let serialized = serde_json::to_string(&msg).map_err(|_| ())?;
        self.broadcast_json(serialized)
    }

    pub fn send(&mut self, id: ClientID, msg: proto::Message) -> Result<(), ()> {
        let serialized = serde_json::to_string(&msg).map_err(|_| ())?;
        self.send_json(id, serialized)
    }
}

impl Drop for WebInterface {
    fn drop(&mut self) {
    }
}