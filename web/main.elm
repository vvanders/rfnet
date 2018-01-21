import Html exposing (..)
import WebSocket

type alias Model = {
    socketAddr: String,
    log: List String
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
            log = []
        },
        Cmd.none
    )

view : Model -> Html Msg
view model =
    div [] 
        (List.map (\l -> div [] [text l]) model.log)

update : Msg -> Model -> (Model, Cmd Msg)
update msg model =
    case msg of
        NewMessage str ->
            ({ model | log = model.log ++ [str] }, Cmd.none)

subscriptions : Model -> Sub Msg
subscriptions model =
    WebSocket.listen model.socketAddr NewMessage