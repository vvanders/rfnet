extern crate rfnet_web;
extern crate rfnet_core;
#[macro_use]
extern crate log;
extern crate fern;
extern crate clap;
extern crate chrono;
extern crate hyper;
extern crate tokio_core;
extern crate futures;

mod rfnet;

use std::sync::mpsc;
use std::sync;

enum Event {
    Log(String, log::Level, String),
    WebsocketMessage(rfnet_web::ClientID, rfnet_web::proto::Command),
    ClientConnected(rfnet_web::ClientID),
    ClientDisconnected(rfnet_web::ClientID)
}

struct LogMapper {
    output: sync::Mutex<mpsc::Sender<Event>>
}

impl log::Log for LogMapper {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let msg = Event::Log(record.target().to_string(), record.level(), format!("{}", record.args()));
        self.output.lock().and_then(|v| {
            v.send(msg).unwrap_or(());
             Ok(())
        }).unwrap_or(());
    }

    fn flush(&self) {
    }
}

fn main() {
    let (event_tx, event_rx) = mpsc::channel();

    let logger: Box<log::Log> = Box::new(LogMapper {
        output: sync::Mutex::new(event_tx.clone())
    });

    fern::Dispatch::new()
        .filter(|metadata| {
            let filter = [ "hyper" ];

            for item in filter.iter() {
                if metadata.target().starts_with(item) {
                    return false
                }
            }

            true
        })
        .format(|out, message, _record| {
            out.finish(format_args!(
                "{} {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                message
            ))
        })
        .level(log::LevelFilter::Trace)
        .chain(std::io::stdout())
        .chain(logger)
        .apply()
        .unwrap();

    let args = clap::App::new("RF NET")
        .version("1.0")
        .author("Val Vanderschaegen <valere.vanderschaegen@gmail.com>")
        .arg(clap::Arg::with_name("http_port")
            .short("p")
            .long("http_port")
            .default_value("8080"))
        .arg(clap::Arg::with_name("ws_port")
            .short("wp")
            .long("ws_port")
            .default_value("8081"))
        .get_matches();

    fn map_ws(msg: rfnet_web::HttpEvent) -> Event {
        match msg {
            rfnet_web::HttpEvent::Message { id, msg } => Event::WebsocketMessage(id, msg),
            rfnet_web::HttpEvent::ClientConnect(id) => Event::ClientConnected(id),
            rfnet_web::HttpEvent::ClientDisconnect(id) => Event::ClientDisconnected(id)
        }
    }

    let http_port = args.value_of("http_port").unwrap_or("8080").parse::<u16>().unwrap_or(8080);
    let ws_port = args.value_of("ws_port").unwrap_or("8081").parse::<u16>().unwrap_or(8081);
    let mut http = rfnet_web::new(http_port, ws_port, event_tx, map_ws).expect("Failed to start http server");

    info!("Started webserver on port {}", http_port);

    let mut logs = vec!();
    let mut rfnet = rfnet::RFNet::new();

    let mut snapshot = rfnet.snapshot();

    loop {
        use rfnet_web::proto::*;

        while let Ok(msg) = event_rx.try_recv() {
            match msg {
                Event::Log(tag, level, msg) => {
                    let msg = Message::Log(LogLine {
                        tag,
                        level: LogLevel::from_log(level),
                        msg
                    });

                    http.broadcast(&msg).unwrap_or(());
                    logs.push(msg);
                },
                Event::WebsocketMessage(_id, msg) => {
                    match msg {
                        Command::ConnectTNC(addr) => rfnet.connect_tcp_tnc(addr.as_str()),
                        Command::Configure(config) => rfnet.configure(config)
                    }
                },
                Event::ClientConnected(id) => {
                    //Update with log state
                    for msg in &logs {
                        http.send(id, msg).unwrap_or(());
                    }

                    //Update with interface state
                    http.send(id, &Message::InterfaceUpdate(rfnet.snapshot())).unwrap_or(());
                }
                _ => {}
            }
        }

        //@todo: Handle error + calc timeout
        rfnet.update_tnc(100).unwrap();

        //If snapshot changed send update
        let next_snapshot = rfnet.snapshot();

        if next_snapshot != snapshot {
            snapshot = next_snapshot.clone();
            http.broadcast(&Message::InterfaceUpdate(next_snapshot)).unwrap_or(());
        }
    }
}
