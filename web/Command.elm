module Command exposing (..)

import Interface exposing (Mode, Configuration, Request)

type Command =
    ConnectTNC(String)
    | Configure(Configuration)