import Html exposing (..)
import WebSocket

type alias Model = {
    socketAddr: String,
    lastLog: String
}

type Msg  = NewMessage String

main : Program Flags Model Msg
main = 
    Html.programWithFlags {
        init = init,
        update = update,
        subscriptions = subscriptions,
        view = view}

type alias Flags = {
    socket: String
}

init : Flags -> (Model, Cmd Msg)
init flags =
    (
        {
            socketAddr = flags.socket,
            lastLog = ""
        },
        Cmd.none
    )

view : Model -> Html Msg
view model =
    div [] [
        text model.lastLog
    ]

update : Msg -> Model -> (Model, Cmd Msg)
update msg model =
    case msg of
        NewMessage str ->
            ({model | lastLog = "Msg: " ++ str }, Cmd.none)

subscriptions : Model -> Sub Msg
subscriptions model =
    WebSocket.listen model.socketAddr NewMessage