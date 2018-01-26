module Event exposing (Event(..), decode)

import Json.Decode exposing (..)

type Event =
    DecodeError String
    | Log String

decode: String -> Event
decode str =
    let
        result = decodeString
            (oneOf [
                decodeLog
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
            (field "tag" string)
            (field "level" string)
            (field "msg" string)
    in
        map Log (field "Log" map_line)