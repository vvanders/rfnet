extern crate rfnet_web;
extern crate rfnet_core;
extern crate log;
extern crate fern;
extern crate clap;
extern crate chrono;

mod rfnet;

use std::sync::mpsc;
use std::sync;

enum Event {
    Log(String, log::Level, String),
    WebsocketMessage(rfnet_web::WebsocketMessage)
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

    fn map_ws(msg: rfnet_web::WebsocketMessage) -> Event {
        Event::WebsocketMessage(msg)
    }

    let http_port = args.value_of("http_port").unwrap_or("8080").parse::<u16>().unwrap_or(8080);
    let ws_port = args.value_of("ws_port").unwrap_or("8081").parse::<u16>().unwrap_or(8081);
    let mut http = rfnet_web::new(http_port, ws_port, event_tx, map_ws).expect("Failed to start http server");

    while let Ok(msg) = event_rx.recv() {
        match msg {
            Event::Log(tag, level, msg) => {
                use rfnet_web::proto::*;

                let msg = LogLine {
                    tag,
                    level: LogLevel::from_log(level),
                    msg
                };

                http.broadcast(Message::Log(msg)).unwrap_or(());
            },
            Event::WebsocketMessage(msg) => {

            }
        }
    }
}
