module Event exposing (Event(..), decode, encodeCmd)

import Json.Decode exposing (..)
import Json.Encode as Encode
import Interface exposing (..)
import Command exposing (..)
import Random

type Event =
    DecodeError String
    | Log String
    | InterfaceUpdate Interface
    | RequestUpdate Request
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

decodeInterfaceType: Decoder Mode
decodeInterfaceType =
    let
        str_decoder = (\s -> case s of
            "Link" -> succeed Link
            "Unconfigured" -> succeed Unconfigured
            _ -> fail "Unsupported value")
        node_decoder = (\s -> case s of 
            "Listening" -> succeed (Node Listening)
            "Idle" -> succeed (Node Idle)
            "Negotiating" -> succeed (Node Negotiating)
            "Established" -> succeed (Node Established)
            "Sending" -> succeed (Node Sending)
            "Receiving" -> succeed (Node Receiving)
            _ -> fail "Unsupported value")
    in
        oneOf [
            ((field "Node" string) |> andThen node_decoder),
            (string |> andThen str_decoder)
        ]

decodeInterface: Decoder Event
decodeInterface =
    let
        map_interface = map2 Interface
            (field "mode" decodeInterfaceType)
            (field "tnc" string)
    in

    map InterfaceUpdate (field "InterfaceUpdate" map_interface)

encodeCmd: Command -> Random.Seed -> (String, Random.Seed)
encodeCmd cmd seed =
    let
        (json_value, new_seed) = 
            case cmd of
                ConnectTNC(addr) ->
                    (Encode.object [ ("ConnectTNC", Encode.string addr) ], seed)
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
                        (Encode.object [ ("Configure", config_obj) ], seed)
                StartRequest request ->
                    let
                        (id, next_seed) = Random.step (Random.int 0 65535) seed
                        method_str = case request.method of
                            GET -> "GET"
                            PUT -> "PUT"
                            POST -> "POST"
                            PATCH -> "PATCH"
                            DELETE -> "DELETE"
                        contents = Encode.object [
                            ("id", Encode.int id),
                            ("url", Encode.string request.url),
                            ("body", Encode.string request.content),
                            ("addr", Encode.string "KI7EST@rfnet.net"),
                            ("method", Encode.string method_str)
                        ]
                    in
                        (Encode.object [ ("Request", contents)], next_seed)
    in
        (Encode.encode 0 json_value, new_seed)

