use std::{
    collections::HashMap,
    env,
    fs::{OpenOptions, create_dir_all},
    io::{ErrorKind, Write},
    net::TcpStream,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use tungstenite::{Error as WsError, Message, connect, stream::MaybeTlsStream};

use super::platform::{
    ChannelCapabilityReport, ChannelCapabilityState, ChannelErrorKind, MessageRoute,
    NormalizedInboundMessage, OutboundMessage, OutboundStreamUpdate, PlatformAdapter,
    PlatformRegistry, StreamDeliveryMode, enabled_platform_names_from_env,
};
use crate::{
    kernel::{
        command::{GatewayCommandBuilder, GatewayMessagePayload},
        envelope::EnvelopeFactory,
    },
    tui::shared::ws_url,
};

pub fn spawn_gateway_channel_runtime() {
    let names = enabled_platform_names_from_env();
    if names.is_empty() {
        return;
    }
    thread::spawn(move || {
        let registry = PlatformRegistry::with_builtin_platforms();
        for name in names {
            let Some(entry) = registry.get(&name) else {
                channel_log(format!("channel {name} is not registered"));
                continue;
            };
            channel_log(format!(
                "channel {} selected label={} native_runtime={}",
                entry.name, entry.label, entry.native_runtime
            ));
            match (entry.factory.as_ref())() {
                Ok(adapter) => spawn_platform_runtime(adapter),
                Err(error) => channel_log(format!(
                    "channel {} unavailable kind={:?} message={}",
                    entry.name, error.kind, error.message
                )),
            }
        }
    });
}

fn spawn_platform_runtime(adapter: Arc<dyn PlatformAdapter>) {
    thread::spawn(move || {
        let (tx, rx) = mpsc::channel();
        let polling_adapter = Arc::clone(&adapter);
        thread::spawn(move || poll_platform_loop(polling_adapter, tx));
        websocket_bridge_loop(adapter, rx);
    });
}

fn poll_platform_loop(
    adapter: Arc<dyn PlatformAdapter>,
    tx: mpsc::Sender<NormalizedInboundMessage>,
) {
    let mut failures = 0usize;
    loop {
        match adapter.poll_updates() {
            Ok(messages) => {
                failures = 0;
                for message in messages {
                    if tx.send(message).is_err() {
                        channel_log(format!("channel {} poll bridge closed", adapter.name()));
                        return;
                    }
                }
            }
            Err(error) => {
                failures = failures.saturating_add(1);
                channel_log(format!(
                    "channel {} poll error kind={:?} message={}",
                    adapter.name(),
                    error.kind,
                    error.message
                ));
                let delay = match error.kind {
                    ChannelErrorKind::SessionExpired => Duration::from_secs(600),
                    ChannelErrorKind::RateLimited => Duration::from_secs(30),
                    ChannelErrorKind::Unavailable | ChannelErrorKind::MissingConfig => {
                        Duration::from_secs(60)
                    }
                    ChannelErrorKind::Retryable => {
                        if failures >= 3 {
                            Duration::from_secs(30)
                        } else {
                            Duration::from_secs(2)
                        }
                    }
                    ChannelErrorKind::Fatal => Duration::from_secs(30),
                };
                thread::sleep(delay);
            }
        }
    }
}

fn websocket_bridge_loop(
    adapter: Arc<dyn PlatformAdapter>,
    inbound_rx: Receiver<NormalizedInboundMessage>,
) {
    let mut routes: HashMap<String, MessageRoute> = HashMap::new();
    let mut streams: HashMap<String, OutboundTurnStream> = HashMap::new();
    loop {
        let url = ws_url();
        channel_log(format!("channel {} ws connect {url}", adapter.name()));
        match connect(url.as_str()) {
            Ok((mut socket, _)) => {
                configure_socket_timeout(&mut socket);
                let gateway = GatewayCommandBuilder::new(EnvelopeFactory::new(format!(
                    "flyflor-cli-channel-{}",
                    adapter.name()
                )));
                let hello = gateway
                    .client_hello(now_millis(), env!("CARGO_PKG_VERSION"))
                    .into_value()
                    .to_string();
                let _ = socket.send(Message::text(hello));
                channel_log(format!("channel {} ws connected", adapter.name()));
                if let Err(error) = run_socket_bridge_session(
                    &adapter,
                    &gateway,
                    &mut socket,
                    &inbound_rx,
                    &mut routes,
                    &mut streams,
                ) {
                    channel_log(format!(
                        "channel {} ws session ended: {error}",
                        adapter.name()
                    ));
                }
            }
            Err(error) => {
                channel_log(format!(
                    "channel {} ws connect failed: {error}",
                    adapter.name()
                ));
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

fn run_socket_bridge_session(
    adapter: &Arc<dyn PlatformAdapter>,
    gateway: &GatewayCommandBuilder,
    socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
    inbound_rx: &Receiver<NormalizedInboundMessage>,
    routes: &mut HashMap<String, MessageRoute>,
    streams: &mut HashMap<String, OutboundTurnStream>,
) -> Result<(), String> {
    loop {
        while let Ok(message) = inbound_rx.try_recv() {
            let route = message.route.clone();
            let payload = inbound_gateway_payload(adapter.as_ref(), &message);
            let envelope = gateway
                .gateway_message_send(now_millis(), payload)
                .into_value()
                .to_string();
            socket
                .send(Message::text(envelope))
                .map_err(|error| error.to_string())?;
            routes.insert(message.id, route);
        }

        match socket.read() {
            Ok(Message::Text(text)) => handle_socket_text(adapter, routes, streams, text.as_ref()),
            Ok(Message::Close(_)) => return Err("socket closed".to_string()),
            Ok(_) => {}
            Err(WsError::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct OutboundTurnStream {
    text: String,
}

fn inbound_gateway_payload(
    adapter: &dyn PlatformAdapter,
    message: &NormalizedInboundMessage,
) -> GatewayMessagePayload {
    let route = &message.route;
    let mut payload = GatewayMessagePayload::new(message.id.clone(), message.text.clone())
        .identity(
            conversation_key(route),
            route.thread_id.clone(),
            route.user_id.clone(),
            route.display_name.clone(),
        )
        .chat_type(route.chat_type.as_gateway_str())
        .metadata(metadata_with_capabilities(
            message.metadata.clone(),
            &adapter.capabilities(),
        ));
    if let Some(context) = message.context.clone() {
        payload = payload.context(context);
    }
    payload
}

fn metadata_with_capabilities(metadata: Value, capabilities: &ChannelCapabilityReport) -> Value {
    let mut metadata = match metadata {
        Value::Object(map) => Value::Object(map),
        other => json!({ "raw": other }),
    };
    if let Some(metadata) = metadata.as_object_mut() {
        let channel = metadata
            .entry("channel".to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if !channel.is_object() {
            *channel = json!({ "raw": channel.clone() });
        }
        if let Some(channel) = channel.as_object_mut() {
            channel.insert("capabilities".to_string(), capabilities.as_metadata());
        }
    }
    metadata
}

fn handle_socket_text(
    adapter: &Arc<dyn PlatformAdapter>,
    routes: &mut HashMap<String, MessageRoute>,
    streams: &mut HashMap<String, OutboundTurnStream>,
    raw: &str,
) {
    let Ok(envelope) = serde_json::from_str::<Value>(raw) else {
        return;
    };
    match envelope.get("type").and_then(Value::as_str) {
        Some("turn.delta") => {
            let Some(message_id) = payload_string(&envelope, "messageId") else {
                return;
            };
            let Some(route) = routes.get(&message_id) else {
                return;
            };
            let delta = payload_string(&envelope, "delta")
                .or_else(|| payload_string(&envelope, "text"))
                .unwrap_or_default();
            if let Some(mode) = adapter.capabilities().supports_stream_mode() {
                let stream = streams.entry(message_id.clone()).or_default();
                stream.text.push_str(&delta);
                if !stream.text.trim().is_empty()
                    && let Err(error) = adapter.stream_update(OutboundStreamUpdate {
                        route: route.clone(),
                        message_id: message_id.clone(),
                        text: stream.text.clone(),
                        mode,
                        final_update: false,
                        metadata: None,
                    })
                {
                    channel_log(format!(
                        "channel {} stream delta failed kind={:?} message={}",
                        adapter.name(),
                        error.kind,
                        error.message
                    ));
                }
            } else if adapter.capabilities().typing != ChannelCapabilityState::Unavailable {
                let _ = adapter.send_typing(route);
            }
        }
        Some("turn.final") => {
            let Some(reply) = envelope
                .get("payload")
                .and_then(|payload| payload.get("reply"))
            else {
                return;
            };
            let Some(message_id) = reply.get("messageId").and_then(Value::as_str) else {
                return;
            };
            let Some(route) = routes.remove(message_id) else {
                return;
            };
            let text = reply
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if text.trim().is_empty() {
                return;
            }
            let metadata = reply.get("metadata").cloned();
            let stream = streams.remove(message_id);
            deliver_turn_final(adapter, route, message_id, text, metadata, stream);
        }
        Some("turn.error") => {
            let message_id = envelope
                .get("payload")
                .and_then(|payload| payload.get("messageId"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            routes.remove(message_id);
            streams.remove(message_id);
        }
        Some("event.publish") => {
            handle_event_publish(adapter, routes, streams, &envelope);
        }
        _ => {}
    }
}

fn deliver_turn_final(
    adapter: &Arc<dyn PlatformAdapter>,
    route: MessageRoute,
    message_id: &str,
    text: String,
    metadata: Option<Value>,
    stream: Option<OutboundTurnStream>,
) {
    let capabilities = adapter.capabilities();
    if let Some(mode) = capabilities.supports_stream_mode() {
        let _ = stream;
        match adapter.stream_update(OutboundStreamUpdate {
            route: route.clone(),
            message_id: message_id.to_string(),
            text: text.clone(),
            mode,
            final_update: true,
            metadata: metadata.clone(),
        }) {
            Ok(_) => return,
            Err(error) => channel_log(format!(
                "channel {} stream final failed kind={:?} message={}",
                adapter.name(),
                error.kind,
                error.message
            )),
        }
    }

    if capabilities.send == ChannelCapabilityState::Unavailable {
        channel_log(format!(
            "channel {} send reply unavailable message_id={message_id}",
            adapter.name()
        ));
        return;
    }

    if let Err(error) = adapter.send_message(OutboundMessage {
        route,
        text,
        reply_to_message_id: Some(message_id.to_string()),
        metadata,
    }) {
        channel_log(format!(
            "channel {} send reply failed kind={:?} message={}",
            adapter.name(),
            error.kind,
            error.message
        ));
    }
}

fn handle_event_publish(
    adapter: &Arc<dyn PlatformAdapter>,
    routes: &HashMap<String, MessageRoute>,
    streams: &mut HashMap<String, OutboundTurnStream>,
    envelope: &Value,
) {
    let Some(message_id) = event_message_id(envelope) else {
        return;
    };
    let Some(route) = routes.get(&message_id) else {
        return;
    };
    let Some(mode @ (StreamDeliveryMode::Card | StreamDeliveryMode::Draft)) =
        adapter.capabilities().supports_stream_mode()
    else {
        return;
    };
    let stream = streams.entry(message_id.clone()).or_default();
    let text = if stream.text.trim().is_empty() {
        event_summary(envelope).unwrap_or_else(|| "runtime event".to_string())
    } else {
        stream.text.clone()
    };
    let _ = adapter.stream_update(OutboundStreamUpdate {
        route: route.clone(),
        message_id,
        text,
        mode,
        final_update: false,
        metadata: envelope.get("payload").cloned(),
    });
}

fn payload_string(envelope: &Value, field: &str) -> Option<String> {
    envelope
        .get("payload")
        .and_then(|payload| payload.get(field))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn event_message_id(envelope: &Value) -> Option<String> {
    for value in [
        envelope.get("payload"),
        envelope
            .get("payload")
            .and_then(|payload| payload.get("event")),
        envelope
            .get("payload")
            .and_then(|payload| payload.get("event"))
            .and_then(|event| event.get("payload")),
        envelope
            .get("payload")
            .and_then(|payload| payload.get("event"))
            .and_then(|event| event.get("data")),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(message_id) = value
            .get("messageId")
            .or_else(|| value.get("message_id"))
            .and_then(Value::as_str)
        {
            return Some(message_id.to_string());
        }
    }
    None
}

fn event_summary(envelope: &Value) -> Option<String> {
    let event = envelope
        .get("payload")
        .and_then(|payload| payload.get("event"))
        .or_else(|| envelope.get("payload"))?;
    event
        .get("type")
        .or_else(|| event.get("eventType"))
        .or_else(|| event.get("name"))
        .and_then(Value::as_str)
        .map(|event_type| format!("runtime event · {event_type}"))
}

fn configure_socket_timeout(socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>) {
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    }
}

fn conversation_key(route: &MessageRoute) -> String {
    format!("{}:{}", route.platform, route.chat_id)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn channel_log(message: impl AsRef<str>) {
    let path = env::var("FLYFLOR_LOG").unwrap_or_else(|_| ".flyflor-cli/logs/dev.log".to_string());
    let path = std::path::PathBuf::from(path);
    if let Some(parent) = path.parent() {
        let _ = create_dir_all(parent);
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{} rust channel {}", now_millis(), message.as_ref());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use crate::tui::gateway::channels::platform::{
        ChannelCapabilityState, ChannelResult, ChatType, PlatformSendOutcome,
    };

    #[test]
    fn conversation_key_keeps_platform_and_chat() {
        let route = MessageRoute {
            platform: "weixin".to_string(),
            chat_id: "user-1".to_string(),
            chat_type: ChatType::Direct,
            user_id: "user-1".to_string(),
            display_name: "user-1".to_string(),
            thread_id: "user-1".to_string(),
        };

        assert_eq!(conversation_key(&route), "weixin:user-1");
    }

    #[test]
    fn mock_ws_inbound_send_envelope_preserves_route_context_and_ask_metadata() {
        let adapter = MockAdapter::new(ChannelCapabilityReport::send_only());
        let message = NormalizedInboundMessage {
            id: "inbound-1".to_string(),
            text: "continue".to_string(),
            route: test_route(),
            context: Some(json!({
                "contextForkId": "fork-1",
                "toolApprovals": {
                    "mcpToolCalls": true,
                    "userToolCalls": true
                }
            })),
            metadata: json!({
                "askAnswer": {
                    "askId": "ask-1",
                    "choiceId": "continue-tools"
                },
                "citizenPermission": {
                    "scope": "continue-tools",
                    "allow": true
                },
                "channel": {
                    "platform": "weixin"
                }
            }),
        };

        let envelope = GatewayCommandBuilder::new(EnvelopeFactory::new("test-channel"))
            .gateway_message_send(
                1_770_000_000_000,
                inbound_gateway_payload(&adapter, &message),
            )
            .into_value();
        let payload = envelope.get("payload").expect("payload");

        assert_eq!(
            envelope.get("type").and_then(Value::as_str),
            Some("gateway.message.send")
        );
        assert_eq!(
            payload.get("text").and_then(Value::as_str),
            Some("continue")
        );
        assert_eq!(
            payload.get("conversationKey").and_then(Value::as_str),
            Some("weixin:chat-1")
        );
        assert_eq!(
            payload.get("threadId").and_then(Value::as_str),
            Some("thread-1")
        );
        assert_eq!(
            payload.get("chatType").and_then(Value::as_str),
            Some("group")
        );
        assert_eq!(
            payload
                .get("user")
                .and_then(|user| user.get("displayName"))
                .and_then(Value::as_str),
            Some("User One")
        );
        assert_eq!(
            payload
                .get("context")
                .and_then(|context| context.get("toolApprovals"))
                .and_then(|approval| approval.get("mcpToolCalls"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|metadata| metadata.get("askAnswer"))
                .and_then(|answer| answer.get("choiceId"))
                .and_then(Value::as_str),
            Some("continue-tools")
        );
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|metadata| metadata.get("citizenPermission"))
                .and_then(|permission| permission.get("allow"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|metadata| metadata.get("channel"))
                .and_then(|channel| channel.get("capabilities"))
                .and_then(|capabilities| capabilities.get("edit"))
                .and_then(Value::as_str),
            Some("unavailable")
        );
    }

    #[test]
    fn mock_ws_outbound_delta_final_and_error_use_send_only_delivery() {
        let adapter = Arc::new(MockAdapter::new(ChannelCapabilityReport::send_only()));
        let adapter_dyn: Arc<dyn PlatformAdapter> = adapter.clone();
        let mut routes = HashMap::from([("message-1".to_string(), test_route())]);
        let mut streams = HashMap::new();

        handle_socket_text(
            &adapter_dyn,
            &mut routes,
            &mut streams,
            r#"{
                "type": "turn.delta",
                "payload": { "messageId": "message-1", "delta": "hel" }
            }"#,
        );
        assert_eq!(adapter.typed.lock().unwrap().len(), 1);

        handle_socket_text(
            &adapter_dyn,
            &mut routes,
            &mut streams,
            r#"{
                "type": "turn.final",
                "payload": {
                    "reply": {
                        "messageId": "message-1",
                        "text": "hello",
                        "metadata": { "answer": true }
                    }
                }
            }"#,
        );
        assert!(routes.is_empty());
        assert_eq!(adapter.sent.lock().unwrap().len(), 1);
        assert_eq!(adapter.sent.lock().unwrap()[0].text, "hello");
        assert_eq!(
            adapter.sent.lock().unwrap()[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("answer"))
                .and_then(Value::as_bool),
            Some(true)
        );

        routes.insert("message-2".to_string(), test_route());
        streams.insert(
            "message-2".to_string(),
            OutboundTurnStream {
                text: "partial".to_string(),
            },
        );
        handle_socket_text(
            &adapter_dyn,
            &mut routes,
            &mut streams,
            r#"{
                "type": "turn.error",
                "payload": { "messageId": "message-2", "message": "failed" }
            }"#,
        );
        assert!(!routes.contains_key("message-2"));
        assert!(!streams.contains_key("message-2"));
    }

    #[test]
    fn mock_ws_outbound_streams_card_updates_and_consumes_event_publish() {
        let adapter = Arc::new(MockAdapter::new(ChannelCapabilityReport {
            send: ChannelCapabilityState::Available,
            typing: ChannelCapabilityState::Available,
            edit: ChannelCapabilityState::Unavailable,
            draft: ChannelCapabilityState::Unavailable,
            card: ChannelCapabilityState::Available,
            media: ChannelCapabilityState::Unavailable,
        }));
        let adapter_dyn: Arc<dyn PlatformAdapter> = adapter.clone();
        let mut routes = HashMap::from([("message-1".to_string(), test_route())]);
        let mut streams = HashMap::new();

        handle_socket_text(
            &adapter_dyn,
            &mut routes,
            &mut streams,
            r#"{
                "type": "turn.delta",
                "payload": { "messageId": "message-1", "delta": "hel" }
            }"#,
        );
        handle_socket_text(
            &adapter_dyn,
            &mut routes,
            &mut streams,
            r#"{
                "type": "event.publish",
                "payload": {
                    "event": {
                        "type": "tool.progress",
                        "payload": { "messageId": "message-1", "step": "running" }
                    }
                }
            }"#,
        );
        handle_socket_text(
            &adapter_dyn,
            &mut routes,
            &mut streams,
            r#"{
                "type": "turn.final",
                "payload": { "reply": { "messageId": "message-1", "text": "hello" } }
            }"#,
        );

        let updates = adapter.stream_updates.lock().unwrap();
        assert_eq!(updates.len(), 3);
        assert!(
            updates
                .iter()
                .all(|update| update.mode == StreamDeliveryMode::Card)
        );
        assert!(!updates[0].final_update);
        assert!(updates[2].final_update);
        assert_eq!(updates[2].text, "hello");
        assert!(adapter.sent.lock().unwrap().is_empty());
    }

    fn test_route() -> MessageRoute {
        MessageRoute {
            platform: "weixin".to_string(),
            chat_id: "chat-1".to_string(),
            chat_type: ChatType::Group,
            user_id: "user-1".to_string(),
            display_name: "User One".to_string(),
            thread_id: "thread-1".to_string(),
        }
    }

    struct MockAdapter {
        capabilities: ChannelCapabilityReport,
        typed: Mutex<Vec<MessageRoute>>,
        sent: Mutex<Vec<OutboundMessage>>,
        stream_updates: Mutex<Vec<OutboundStreamUpdate>>,
    }

    impl MockAdapter {
        fn new(capabilities: ChannelCapabilityReport) -> Self {
            Self {
                capabilities,
                typed: Mutex::new(Vec::new()),
                sent: Mutex::new(Vec::new()),
                stream_updates: Mutex::new(Vec::new()),
            }
        }
    }

    impl PlatformAdapter for MockAdapter {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn capabilities(&self) -> ChannelCapabilityReport {
            self.capabilities.clone()
        }

        fn poll_updates(&self) -> ChannelResult<Vec<NormalizedInboundMessage>> {
            Ok(Vec::new())
        }

        fn send_typing(&self, route: &MessageRoute) -> ChannelResult<()> {
            self.typed.lock().unwrap().push(route.clone());
            Ok(())
        }

        fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
            self.sent.lock().unwrap().push(message);
            Ok(PlatformSendOutcome {
                message_id: Some("mock-sent".to_string()),
            })
        }

        fn stream_update(
            &self,
            update: OutboundStreamUpdate,
        ) -> ChannelResult<PlatformSendOutcome> {
            self.stream_updates.lock().unwrap().push(update);
            Ok(PlatformSendOutcome {
                message_id: Some("mock-stream".to_string()),
            })
        }
    }
}
