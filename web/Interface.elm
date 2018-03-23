module Interface exposing (..)

type alias Interface = {
    mode: Mode,
    tnc: String
}

type Mode = 
    Node
    | Link
    | Unconfigured

type ConfigurationMode = ConfigNode | ConfigLink LinkConfig

type alias LinkConfig = {
    link_width: Int,
    fec: Bool,
    retry: Bool,
    broadcast_rate: Int
}

type alias RetryConfig = {
    delay_ms: Int,
    bps: Int,
    bps_scale: Float,
    retry_attempts: Int
}

type alias Configuration = {
    mode: ConfigurationMode,
    callsign: String,
    retry: RetryConfig
}