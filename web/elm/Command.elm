module Command exposing (..)

import Interface exposing (Mode, Configuration, Request, HttpMethod)

type Command =
    ConnectTNC(String)
    | Configure(Configuration)
    | StartRequest Request