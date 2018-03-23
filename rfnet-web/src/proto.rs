use log;

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

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Interface {
    pub mode: Mode,
    pub tnc: String
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Mode {
    Node,
    Link,
    Unconfigured
}

#[derive(Serialize, Deserialize)]
pub enum Message {
    Log(LogLine),
    InterfaceUpdate(Interface)
}

#[derive(Serialize, Deserialize)]
pub struct ConfigureRetry {
    pub delay_ms: usize,
    pub bps: usize,
    pub bps_scale: f32,
    pub retry_attempts: usize
}

#[derive(Serialize, Deserialize)]
pub enum ConfigureMode {
    Node,
    Link(ConfigureLink)
}

#[derive(Serialize, Deserialize)]
pub struct ConfigureLink {
    pub link_width: u16,
    pub fec: bool,
    pub retry: bool,
    pub broadcast_rate: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct Configuration {
    pub callsign: String,
    pub retry_config: ConfigureRetry,
    pub mode: ConfigureMode
}

#[derive(Serialize, Deserialize)]
pub enum Command {
    ConnectTNC(String),
    Configure(Configuration)
}