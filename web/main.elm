import Html exposing (..)
import WebSocket

import Event exposing (..)

type alias Model = {
    socketAddr: String,
    log: List String
}

main : Program Flags Model Event
main = 
    Html.programWithFlags {
        init = init,
        update = update,
        subscriptions = subscriptions,
        view = view}

type alias Flags = {
    socket: String
}

init : Flags -> (Model, Cmd Event)
init flags =
    (
        {
            socketAddr = flags.socket,
            log = []
        },
        Cmd.none
    )

view : Model -> Html Event
view model =
    div [] 
        (List.map (\l -> div [] [text l]) model.log)

update : Event -> Model -> (Model, Cmd Event)
update msg model =
    case msg of
        Log str ->
            ({ model | log = model.log ++ [str] }, Cmd.none)
        DecodeError str ->
            ({ model | log = model.log ++ ["Decode Error: " ++ str] }, Cmd.none)

subscriptions : Model -> Sub Event
subscriptions model =
    WebSocket.listen model.socketAddr (\str -> decode str)