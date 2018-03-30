use rfnet_core::node;
use rfnet_core::message;

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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum NodeState {
    Listening,
    Idle,
    Negotiating,
    Established,
    Sending,
    Receiving
}

impl ::std::convert::From<node::ClientState> for NodeState {
    fn from(state: node::ClientState) -> NodeState {
        use self::node::ClientState;

        match state {
            ClientState::Listening => NodeState::Listening,
            ClientState::Idle => NodeState::Idle,
            ClientState::Negotiating => NodeState::Negotiating,
            ClientState::Established => NodeState::Established,
            ClientState::Sending => NodeState::Sending,
            ClientState::Receiving => NodeState::Receiving
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Mode {
    Node(NodeState),
    Link,
    Unconfigured
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Response {
    pub id: u32,
    pub code: u16,
    pub content: String
}

#[derive(Serialize, Deserialize)]
pub enum Message {
    Log(LogLine),
    InterfaceUpdate(Interface),
    Response(Response)
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

#[derive(Serialize, Deserialize, Clone)]
pub enum HTTPMethod {
    GET,
    PUT,
    PATCH,
    POST,
    DELETE
}

impl ::std::convert::From<message::RESTMethod> for HTTPMethod {
    fn from(method: message::RESTMethod) -> HTTPMethod {
        match method {
            message::RESTMethod::GET => HTTPMethod::GET,
            message::RESTMethod::PUT => HTTPMethod::PUT,
            message::RESTMethod::PATCH => HTTPMethod::PATCH,
            message::RESTMethod::POST => HTTPMethod::POST,
            message::RESTMethod::DELETE => HTTPMethod::DELETE
        }
    }
}

impl ::std::convert::From<HTTPMethod> for message::RESTMethod {
    fn from(method: HTTPMethod) -> message::RESTMethod {
        match method {
            HTTPMethod::GET => message::RESTMethod::GET,
            HTTPMethod::PUT => message::RESTMethod::PUT,
            HTTPMethod::PATCH => message::RESTMethod::PATCH,
            HTTPMethod::POST => message::RESTMethod::POST,
            HTTPMethod::DELETE => message::RESTMethod::DELETE
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub id: u32,
    pub addr: String,
    pub url: String,
    pub method: HTTPMethod,
    pub content: String
}

#[derive(Serialize, Deserialize)]
pub enum Command {
    ConnectTNC(String),
    Configure(Configuration),
    Request(Request)
}