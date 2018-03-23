module Command exposing (..)

import Interface exposing (Mode, Configuration)

type Command =
    ConnectTNC(String)
    | Configure(Configuration)