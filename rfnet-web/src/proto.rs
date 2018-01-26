use log;

#[derive(Serialize, Deserialize)]
pub enum Mode {
    Node,
    Link
}

#[derive(Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error
}

impl LogLevel {
    pub fn from_log(level: log::Level) -> LogLevel {
        match level {
            log::Level::Trace => LogLevel::Trace,
            log::Level::Debug => LogLevel::Debug,
            log::Level::Info => LogLevel::Info,
            log::Level::Warn => LogLevel::Warn,
            log::Level::Error => LogLevel::Error,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct LogLine {
    pub tag: String,
    pub level: LogLevel,
    pub msg: String
}

#[derive(Serialize, Deserialize)]
pub enum Message {
    Log(LogLine),
    SetCallsign(String),
    SetMode(Mode)
}