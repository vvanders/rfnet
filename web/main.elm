import Html exposing (..)
import Html.Attributes exposing (type_, value)
import Html.Events exposing (onClick, onInput, onCheck)
import WebSocket

import Event exposing (..)
import Interface exposing (..)
import Command exposing (..)

type HttpMethod =
    GET
    | POST
    | PATCH
    | PUT
    | DELETE

type alias Request = {
    url: String,
    method: HttpMethod,
    content: String
}

type alias Model = {
    socketAddr: String,
    log: List String,
    interface: Interface,
    config: Configuration,
    request: Request
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
            log = [],
            interface = {
                mode = Unconfigured,
                tnc = "Disconnected"
            },
            config = {
                mode = ConfigNode,
                callsign = "CALLSIGN",
                retry = {
                    bps = 1200,
                    bps_scale = 1.0,
                    delay_ms = 0,
                    retry_attempts = 5
                }
            },
            request = {
                url = "",
                method = GET,
                content = ""
            }
        },
        Cmd.none
    )

view : Model -> Html Event
view model =
    div [] [
        viewConfig model.config,
        viewInterface model.interface,
        br [] [],
        div [] (List.map (\l -> div [] [text l]) model.log)
    ]

numberInput : String -> Int -> (Int -> Command) -> Html Event
numberInput field num_value cmd =
    let
        event = (\v -> case String.toInt v of
            Ok i -> Command (cmd i)
            Err e -> DecodeError e)
        changed = onInput event
    in
        label [] [
            text field,
            input [ type_ "text", value (toString num_value), changed ] []
        ]

boolInput : String -> Bool -> (Bool -> Command) -> Html Event
boolInput field bool_value cmd =
    let
        changed = onCheck (\v -> Command (cmd v))
    in
        label [] [
            text field,
            input [ type_ "checkbox", changed, Html.Attributes.checked bool_value ] []
        ]

floatInput : String -> Float -> (Float -> Command) -> Html Event
floatInput field f_value cmd =
    let
        event = (\v -> case String.toFloat v of
            Ok f -> Command (cmd f)
            Err e -> DecodeError e)
        changed = onInput event
    in
        label [] [
            text field,
            input [ type_ "text", value (toString f_value), changed ] []
        ]

textInput : String -> String -> (String -> Command) -> Html Event
textInput field s_value cmd =
    label [] [
        text field,
        input [ type_ "text", value s_value, onInput (\v -> Command (cmd v)) ] []
    ]

viewConfig : Configuration -> Html Event
viewConfig config =
    let
        retry_config = config.retry
        mode = case config.mode of
            ConfigNode ->
                div [] [
                    text "Node"
                ]
            ConfigLink link_config ->
                div[] [
                    text "Link",
                    numberInput "link_width" link_config.link_width (\v -> Configure { config | mode = ConfigLink { link_config | link_width = v }}),
                    boolInput "fec" link_config.fec (\v -> Configure { config | mode = ConfigLink { link_config | fec = v }}),
                    boolInput "retry" link_config.retry (\v -> Configure { config | mode = ConfigLink { link_config | retry = v }}),
                    numberInput "broadcast_rate" link_config.broadcast_rate (\v -> Configure { config | mode = ConfigLink { link_config | broadcast_rate = v }})
                ]
        retry = [
            numberInput "bps" config.retry.bps (\v -> Configure { config | retry = { retry_config | bps = v }}),
            floatInput "bps_scale" config.retry.bps_scale (\v -> Configure { config | retry = { retry_config | bps_scale = v }}),
            numberInput "delay_ms" config.retry.delay_ms (\v -> Configure { config | retry = { retry_config | delay_ms = v }}),
            numberInput "retry_attempts" config.retry.retry_attempts (\v -> Configure { config | retry = { retry_config | retry_attempts = v }})
        ]
        node_select = Command (Configure { config | mode = ConfigNode })
        link_select = Command (Configure { config | mode = ConfigLink {
            link_width = 255,
            fec = True,
            retry = True,
            broadcast_rate = 30
         } })
    in
        div [] [
            fieldset [] [
                textInput "callsign" config.callsign (\v -> Configure { config | callsign = v })
            ],
            fieldset [] [
                label [] [
                    input [ type_ "radio", Html.Attributes.name "mode", onClick node_select ] [],
                    text "Node"
                ],
                label [] [
                    input [ type_ "radio", Html.Attributes.name "mode", onClick link_select ] [],
                    text "Link"
                ]
            ],
            fieldset [] retry,
            fieldset [] [
                mode
            ]
        ]

viewNode : NodeState -> Html Event
viewNode state =
    div [] [
        text ("Node - " ++ (toString state))
    ]

viewInterface : Interface -> Html Event
viewInterface interface =
    let
        mode = case interface.mode of
            Node state -> viewNode state
            Link -> text "Link"
            Unconfigured -> text "Unconfigured"
    in
        div [] [
            text ("TNC: " ++ interface.tnc),
            mode,
            button [ onClick (Command (ConnectTNC "127.0.0.1:8001")) ] [ text "Connect" ]
        ]

update : Event -> Model -> (Model, Cmd Event)
update msg model =
    case msg of
        Log str ->
            ({ model | log = [str] ++ model.log }, Cmd.none)
        DecodeError str ->
            ({ model | log = model.log ++ ["Decode Error: " ++ str] }, Cmd.none)
        InterfaceUpdate interface ->
            ({ model | interface = interface }, Cmd.none)
        Command cmd ->
            case cmd of 
                Configure cfg ->
                    ({ model | config = cfg }, WebSocket.send model.socketAddr (encodeCmd cmd))
                other -> 
                    (model, WebSocket.send model.socketAddr (encodeCmd cmd))


subscriptions : Model -> Sub Event
subscriptions model =
    WebSocket.listen model.socketAddr (\str -> decode str)