import Html exposing (..)

type alias Model = ()

type Msg  = None

main : Program Never () Msg
main = 
    Html.program {
        init = ((), Cmd.none),
        update = update,
        subscriptions = subscriptions,
        view = view}

view : Model -> Html Msg
view model =
    div [] []

update : Msg -> Model -> (Model, Cmd Msg)
update msg model =
    (model, Cmd.none)

subscriptions : Model -> Sub Msg
subscriptions model =
    Sub.none