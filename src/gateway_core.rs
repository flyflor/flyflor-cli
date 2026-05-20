use std::{
    io::ErrorKind,
    net::TcpStream,
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use tungstenite::{
    Message, WebSocket, connect,
    stream::MaybeTlsStream,
};

use crate::protocol::{
    AckPayload, AskMetadata, CapabilityCatalogPayload, ControlErrorPayload, EVENT_PROTOCOL,
    EventPublishPayload, GatewayMessageSendPayload, GatewayStatusEnvelopePayload, GatewayStatusPayload, LoopMetadata,
    PlanningMetadata, ServerHelloPayload, SubscriptionSnapshot, TurnDeltaPayload, TurnErrorPayload,
    TurnFinalPayload, WS_PROTOCOL, WsEnvelope,
};

type WsStream = WebSocket<MaybeTlsStream<TcpStream>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionPhase {
    Idle,
    Connecting,
    OpenWaitingHello,
    Ready,
    Degraded,
    Reconnecting,
    Closed,
}

#[derive(Clone, Debug, Default)]
pub struct SessionTurn {
    pub request_id: String,
    pub message_id: String,
    pub user_text: String,
    pub assistant_text: String,
    pub status: TurnStatus,
    pub ask_snapshot: Option<AskMetadata>,
    pub planning_snapshot: Option<PlanningMetadata>,
    pub loop_snapshot: Option<LoopMetadata>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub enum TurnStatus {
    #[default]
    Pending,
    Streaming,
    Final,
    Failed,
}

#[derive(Clone, Debug)]
pub struct ConnectionState {
    pub phase: ConnectionPhase,
    pub client_id: Option<String>,
    pub connected_at: Option<String>,
    pub last_pong_at: Option<Instant>,
    pub last_error: Option<String>,
    pub ws_url: String,
    pub hello_snapshot: Option<ServerHelloPayload>,
    pub gateway_status_snapshot: Option<GatewayStatusPayload>,
    pub capability_catalog_snapshot: Option<CapabilityCatalogPayload>,
    pub subscription_snapshot: Vec<SubscriptionSnapshot>,
}

impl ConnectionState {
    pub fn new(ws_url: String) -> Self {
        Self {
            phase: ConnectionPhase::Idle,
            client_id: None,
            connected_at: None,
            last_pong_at: None,
            last_error: None,
            ws_url,
            hello_snapshot: None,
            gateway_status_snapshot: None,
            capability_catalog_snapshot: None,
            subscription_snapshot: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum GatewayCoreCommand {
    Connect,
    Disconnect,
    SendMessage {
        text: String,
        user_id: String,
        display_name: Option<String>,
    },
    Ping,
    RefreshStatus,
    RefreshCatalog,
    SubscribeEvents {
        types: Vec<String>,
        classes: Vec<String>,
    },
}

#[derive(Clone, Debug)]
pub enum GatewayCoreEvent {
    ConnectionChanged(ConnectionState),
    ServerHello(ServerHelloPayload),
    Ack(AckPayload),
    GatewayStatusSnapshot(GatewayStatusPayload),
    CapabilityCatalogSnapshot(CapabilityCatalogPayload),
    TurnDelta {
        request_id: Option<String>,
        payload: TurnDeltaPayload,
    },
    TurnFinal {
        request_id: Option<String>,
        payload: TurnFinalPayload,
    },
    TurnError {
        request_id: Option<String>,
        payload: TurnErrorPayload,
    },
    ControlError(ControlErrorPayload),
    RuntimeEvent {
        request_id: Option<String>,
        payload: EventPublishPayload,
    },
}

pub struct GatewayCore {
    pub commands: Sender<GatewayCoreCommand>,
    pub events: Receiver<GatewayCoreEvent>,
}

pub fn spawn_gateway_core(ws_url: String) -> GatewayCore {
    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    thread::spawn(move || run_gateway_thread(ws_url, command_rx, event_tx));

    GatewayCore {
        commands: command_tx,
        events: event_rx,
    }
}

fn run_gateway_thread(
    ws_url: String,
    command_rx: Receiver<GatewayCoreCommand>,
    event_tx: Sender<GatewayCoreEvent>,
) {
    let mut state = ConnectionState::new(ws_url.clone());
    let mut socket: Option<WsStream> = None;
    let mut active = true;

    while active {
        if let Some(ws) = socket.as_mut() {
            drain_socket(ws, &mut state, &event_tx);
        }

        match command_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(command) => match command {
                GatewayCoreCommand::Connect => {
                    state.phase = ConnectionPhase::Connecting;
                    let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                    match connect(&ws_url) {
                        Ok((mut ws, _)) => {
                            configure_stream(&mut ws);
                            state.phase = ConnectionPhase::OpenWaitingHello;
                            let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                            send_client_bootstrap(&mut ws);
                            socket = Some(ws);
                        }
                        Err(error) => {
                            state.phase = ConnectionPhase::Degraded;
                            state.last_error = Some(error.to_string());
                            let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                        }
                    }
                }
                GatewayCoreCommand::Disconnect => {
                    if let Some(mut ws) = socket.take() {
                        let _ = ws.close(None);
                    }
                    state.phase = ConnectionPhase::Closed;
                    let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                    active = false;
                }
                GatewayCoreCommand::SendMessage {
                    text,
                    user_id,
                    display_name,
                } => {
                    if let Some(ws) = socket.as_mut() {
                        let request_id = next_id("req");
                        let payload = GatewayMessageSendPayload {
                            text,
                            id: Some(next_id("message")),
                            chat_id: Some(user_id.clone()),
                            thread_id: None,
                            user: Some(crate::protocol::GatewayUserPayload {
                                id: Some(user_id),
                                display_name,
                            }),
                        };
                        let json = build_envelope(
                            "gateway.message.send",
                            Some(serde_json::to_value(payload).unwrap_or(Value::Null)),
                            Some(request_id),
                        );
                        let _ = ws.send(Message::Text(json.into()));
                    }
                }
                GatewayCoreCommand::Ping => {
                    if let Some(ws) = socket.as_mut() {
                        let json = build_envelope("ping", Some(json!({})), None);
                        let _ = ws.send(Message::Text(json.into()));
                    }
                }
                GatewayCoreCommand::RefreshStatus => {
                    if let Some(ws) = socket.as_mut() {
                        let json = build_envelope("gateway.status.get", None, None);
                        let _ = ws.send(Message::Text(json.into()));
                    }
                }
                GatewayCoreCommand::RefreshCatalog => {
                    if let Some(ws) = socket.as_mut() {
                        let json = build_envelope("capability.catalog.get", None, None);
                        let _ = ws.send(Message::Text(json.into()));
                    }
                }
                GatewayCoreCommand::SubscribeEvents { types, classes } => {
                    if let Some(ws) = socket.as_mut() {
                        let json = build_envelope(
                            "event.subscribe",
                            Some(json!({ "types": types, "classes": classes })),
                            None,
                        );
                        let _ = ws.send(Message::Text(json.into()));
                    }
                }
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                active = false;
            }
        }
    }
}

fn configure_stream(ws: &mut WsStream) {
    match ws.get_mut() {
        MaybeTlsStream::Plain(stream) => {
            let _ = stream.set_nonblocking(true);
        }
        #[allow(unreachable_patterns)]
        _ => {}
    }
}

fn send_client_bootstrap(ws: &mut WsStream) {
    let hello = build_envelope(
        "client.hello",
        Some(json!({
            "name": "flyflor",
            "version": env!("CARGO_PKG_VERSION"),
        })),
        None,
    );
    let _ = ws.send(Message::Text(hello.into()));

    let subscribe = build_envelope(
        "event.subscribe",
        Some(json!({
            "types": [
                "gateway.message.received",
                "channel.link.changed",
                "channel.error",
                "memory.ask.recorded",
                "memory.ask.answered",
                "memory.task_plan.written",
                "memory.context_fork.written",
                "memory.scene_record.written",
                "cttl.long_horizon_loop.paused",
                "cttl.long_horizon_loop.resumed",
                "cttl.loop.guard.blocked"
            ],
            "classes": ["gateway", "memory", "cttl"]
        })),
        None,
    );
    let _ = ws.send(Message::Text(subscribe.into()));

    let status = build_envelope("gateway.status.get", None, None);
    let _ = ws.send(Message::Text(status.into()));

    let catalog = build_envelope("capability.catalog.get", None, None);
    let _ = ws.send(Message::Text(catalog.into()));
}

fn drain_socket(ws: &mut WsStream, state: &mut ConnectionState, event_tx: &Sender<GatewayCoreEvent>) {
    loop {
        match ws.read() {
            Ok(message) => match message {
                Message::Text(text) => handle_text(text.as_ref(), state, event_tx),
                Message::Pong(_) => {
                    state.last_pong_at = Some(Instant::now());
                    if state.phase != ConnectionPhase::Ready {
                        state.phase = ConnectionPhase::Ready;
                    }
                    let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                }
                Message::Close(_) => {
                    state.phase = ConnectionPhase::Closed;
                    let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                    break;
                }
                _ => {}
            },
            Err(tungstenite::Error::Io(error))
                if error.kind() == ErrorKind::WouldBlock || error.kind() == ErrorKind::TimedOut =>
            {
                break;
            }
            Err(tungstenite::Error::Protocol(_)) => break,
            Err(error) => {
                state.phase = ConnectionPhase::Degraded;
                state.last_error = Some(error.to_string());
                let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                break;
            }
        }
    }
}

fn handle_text(text: &str, state: &mut ConnectionState, event_tx: &Sender<GatewayCoreEvent>) {
    let Ok(envelope) = serde_json::from_str::<WsEnvelope>(text) else {
        return;
    };

    if envelope.protocol == EVENT_PROTOCOL {
        if envelope.message_type == "event.publish" {
            if let Some(payload) = envelope.payload {
                if let Ok(parsed) = serde_json::from_value::<EventPublishPayload>(payload) {
                    let _ = event_tx.send(GatewayCoreEvent::RuntimeEvent {
                        request_id: envelope.request_id,
                        payload: parsed,
                    });
                }
            }
        }
        return;
    }

    if envelope.protocol != WS_PROTOCOL {
        return;
    }

    match envelope.message_type.as_str() {
        "server.hello" => {
            if let Some(payload) = envelope.payload {
                if let Ok(parsed) = serde_json::from_value::<ServerHelloPayload>(payload) {
                    state.phase = ConnectionPhase::Ready;
                    state.client_id = Some(parsed.client_id.clone());
                    state.connected_at = Some(parsed.connected_at.clone());
                    state.gateway_status_snapshot = Some(parsed.status.clone());
                    state.hello_snapshot = Some(parsed.clone());
                    let _ = event_tx.send(GatewayCoreEvent::ServerHello(parsed));
                    let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                }
            }
        }
        "ack" => {
            if let Some(payload) = envelope.payload {
                if let Ok(parsed) = serde_json::from_value::<AckPayload>(payload) {
                    state.subscription_snapshot = parsed.subscriptions.clone();
                    let _ = event_tx.send(GatewayCoreEvent::Ack(parsed));
                    let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                }
            }
        }
        "gateway.status.snapshot" => {
            if let Some(payload) = envelope.payload {
                if let Ok(parsed) = serde_json::from_value::<GatewayStatusEnvelopePayload>(payload) {
                    state.gateway_status_snapshot = Some(parsed.status.clone());
                    let _ = event_tx.send(GatewayCoreEvent::GatewayStatusSnapshot(parsed.status));
                    let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                }
            }
        }
        "capability.catalog.snapshot" => {
            if let Some(payload) = envelope.payload {
                if let Ok(parsed) = serde_json::from_value::<CapabilityCatalogPayload>(payload) {
                    state.capability_catalog_snapshot = Some(parsed.clone());
                    let _ = event_tx.send(GatewayCoreEvent::CapabilityCatalogSnapshot(parsed));
                    let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                }
            }
        }
        "turn.delta" => {
            if let Some(payload) = envelope.payload {
                if let Ok(parsed) = serde_json::from_value::<TurnDeltaPayload>(payload) {
                    let _ = event_tx.send(GatewayCoreEvent::TurnDelta {
                        request_id: envelope.request_id,
                        payload: parsed,
                    });
                }
            }
        }
        "turn.final" => {
            if let Some(payload) = envelope.payload {
                if let Ok(parsed) = serde_json::from_value::<TurnFinalPayload>(payload) {
                    let _ = event_tx.send(GatewayCoreEvent::TurnFinal {
                        request_id: envelope.request_id,
                        payload: parsed,
                    });
                }
            }
        }
        "turn.error" => {
            if let Some(payload) = envelope.payload {
                if let Ok(parsed) = serde_json::from_value::<TurnErrorPayload>(payload) {
                    let _ = event_tx.send(GatewayCoreEvent::TurnError {
                        request_id: envelope.request_id,
                        payload: parsed,
                    });
                }
            }
        }
        "error" => {
            if let Some(payload) = envelope.payload {
                if let Ok(parsed) = serde_json::from_value::<ControlErrorPayload>(payload) {
                    state.last_error = Some(parsed.message.clone());
                    let _ = event_tx.send(GatewayCoreEvent::ControlError(parsed));
                    let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
                }
            }
        }
        "pong" => {
            state.last_pong_at = Some(Instant::now());
            let _ = event_tx.send(GatewayCoreEvent::ConnectionChanged(state.clone()));
        }
        _ => {}
    }
}

fn build_envelope(message_type: &str, payload: Option<Value>, request_id: Option<String>) -> String {
    serde_json::to_string(&json!({
        "protocol": WS_PROTOCOL,
        "id": next_id("env"),
        "type": message_type,
        "at": now_iso_like(),
        "requestId": request_id,
        "payload": payload
    }))
    .unwrap_or_else(|_| String::from("{}"))
}

fn next_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis();
    format!("{prefix}-{millis}")
}

fn now_iso_like() -> String {
    next_id("ts")
}
