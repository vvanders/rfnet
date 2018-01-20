extern crate rfnet_web;
extern crate log;
extern crate fern;
extern crate clap;
extern crate chrono;

fn main() {
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
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Trace)
        .chain(std::io::stdout())
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

    let http_port = args.value_of("http_port").unwrap_or("8080").parse::<u16>().unwrap_or(8080);
    let ws_port = args.value_of("ws_port").unwrap_or("8081").parse::<u16>().unwrap_or(8081);
    let _http = rfnet_web::new(http_port, ws_port);
}
