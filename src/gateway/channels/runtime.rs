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
    ChannelErrorKind, MessageRoute, NormalizedInboundMessage, OutboundMessage, PlatformAdapter,
    PlatformRegistry, enabled_platform_names_from_env,
};
use crate::{
    tui::kernel::{
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
                "channel {} selected label={} implemented={}",
                entry.name, entry.label, entry.implemented
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
) -> Result<(), String> {
    loop {
        while let Ok(message) = inbound_rx.try_recv() {
            let route = message.route.clone();
            let payload = GatewayMessagePayload::new(message.id.clone(), message.text.clone())
                .identity(
                    conversation_key(&route),
                    route.thread_id.clone(),
                    route.user_id.clone(),
                    route.display_name.clone(),
                )
                .chat_type(route.chat_type.as_gateway_str())
                .metadata(message.metadata.clone());
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
            Ok(Message::Text(text)) => handle_socket_text(adapter, routes, text.as_ref()),
            Ok(Message::Close(_)) => return Err("socket closed".to_string()),
            Ok(_) => {}
            Err(WsError::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn handle_socket_text(
    adapter: &Arc<dyn PlatformAdapter>,
    routes: &mut HashMap<String, MessageRoute>,
    raw: &str,
) {
    let Ok(envelope) = serde_json::from_str::<Value>(raw) else {
        return;
    };
    match envelope.get("type").and_then(Value::as_str) {
        Some("turn.delta") => {
            if let Some(message_id) = envelope
                .get("payload")
                .and_then(|payload| payload.get("messageId"))
                .and_then(Value::as_str)
                && let Some(route) = routes.get(message_id)
            {
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
        Some("turn.error") => {
            let message_id = envelope
                .get("payload")
                .and_then(|payload| payload.get("messageId"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            routes.remove(message_id);
        }
        _ => {}
    }
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
    use crate::gateway::channels::platform::ChatType;

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
}
