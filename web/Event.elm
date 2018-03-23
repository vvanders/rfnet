module Event exposing (Event(..), decode, encodeCmd)

import Json.Decode exposing (..)
import Json.Encode as Encode
import Interface exposing (..)
import Command exposing(..)

type Event =
    DecodeError String
    | Log String
    | InterfaceUpdate Interface
    | Command Command

decode: String -> Event
decode str =
    let
        result = decodeString
            (oneOf [
                decodeLog,
                decodeInterface
            ])
            str
    in
        case result of
            Ok event -> event
            Err msg -> DecodeError msg


decodeLog: Decoder Event
decodeLog =
    let
        map_line = map3 (\t l m -> t ++ " [" ++ l ++ "] " ++ m)
            (field "level" string)
            (field "tag" string)
            (field "msg" string)
    in
        map Log (field "Log" map_line)

decodeInterfaceType: String -> Decoder Mode
decodeInterfaceType value =
    case value of
        "Node" ->
            succeed Node
        "Link" ->
            succeed Link
        "Unconfigured" ->
            succeed Unconfigured
        _ -> 
            fail "Unsupported value"

decodeInterface: Decoder Event
decodeInterface =
    let
        map_interface = map2 Interface
            ((field "mode" string) |> andThen decodeInterfaceType)
            (field "tnc" string)
    in

    map InterfaceUpdate (field "InterfaceUpdate" map_interface)

encodeCmd: Command -> String
encodeCmd cmd =
    let
        json_value = 
            case cmd of
                ConnectTNC(addr) ->
                    Encode.object [ ("ConnectTNC", Encode.string addr) ]
                Configure(config) ->
                    let
                        config_mode = case config.mode of
                            ConfigNode -> Encode.string "Node"
                            ConfigLink link -> 
                                Encode.object [
                                    ("Link",
                                        Encode.object [
                                            ("link_width", Encode.int link.link_width),
                                            ("fec", Encode.bool link.fec),
                                            ("retry", Encode.bool link.retry),
                                            ("broadcast_rate", Encode.int link.broadcast_rate)
                                        ]
                                    )
                            ]
                        config_retry = Encode.object [
                            ("bps", Encode.int config.retry.bps),
                            ("bps_scale", Encode.float config.retry.bps_scale),
                            ("delay_ms", Encode.int config.retry.delay_ms),
                            ("retry_attempts", Encode.int config.retry.retry_attempts)
                        ]
                        config_obj = Encode.object [
                            ("retry_config", config_retry),
                            ("callsign", Encode.string config.callsign),
                            ("mode", config_mode)
                        ]
                    in
                        Encode.object [ ("Configure", config_obj) ]
    in
        Encode.encode 0 json_value

