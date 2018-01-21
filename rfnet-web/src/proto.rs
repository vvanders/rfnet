#[derive(Serialize, Deserialize)]
pub enum Mode {
    Node,
    Link
}

#[derive(Serialize, Deserialize)]
pub enum Message {
    Log(String),
    SetCallsign(String),
    SetMode(Mode)
}