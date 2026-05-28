use std::{
    collections::HashSet,
    env,
    io::{ErrorKind, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

use super::platform::{
    ChannelCapabilityReport, ChannelCapabilityState, ChannelError, ChannelResult, ChatType,
    MessageRoute, NormalizedInboundMessage, OutboundMessage, PlatformAdapter, PlatformSendOutcome,
};

const IRC_CONNECT_TIMEOUT_MS: u64 = 5_000;
const IRC_READ_TIMEOUT_MS: u64 = 100;
const IRC_MAX_LINE_BYTES: usize = 440;

pub struct IrcAdapter {
    nickname: String,
    channel: String,
    allowed_users: HashSet<String>,
    connection: Mutex<IrcConnection>,
}

impl IrcAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let server = env::var("IRC_SERVER")
            .or_else(|_| env::var("FLYFLOR_IRC_SERVER"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if server.is_empty() {
            return Err(ChannelError::missing_config(
                "IRC_SERVER is required for the irc channel",
            ));
        }
        let nickname = env::var("IRC_NICKNAME")
            .or_else(|_| env::var("IRC_NICK"))
            .or_else(|_| env::var("FLYFLOR_IRC_NICKNAME"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if nickname.is_empty() {
            return Err(ChannelError::missing_config(
                "IRC_NICKNAME is required for the irc channel",
            ));
        }
        let channel = env::var("IRC_CHANNEL")
            .or_else(|_| env::var("IRC_CHANNELS").map(|value| first_csv_value(&value)))
            .unwrap_or_default()
            .trim()
            .to_string();
        if channel.is_empty() {
            return Err(ChannelError::missing_config(
                "IRC_CHANNEL is required for the irc channel",
            ));
        }
        let port = env::var("IRC_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(6667);
        let password = env::var("IRC_SERVER_PASSWORD")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let connection = IrcConnection::connect(&server, port, &nickname, &channel, password)?;
        Ok(Self {
            nickname,
            channel,
            allowed_users: env_set("IRC_ALLOWED_USERS"),
            connection: Mutex::new(connection),
        })
    }

    fn normalize_line(&self, line: &str) -> Option<NormalizedInboundMessage> {
        let message = parse_privmsg(line)?;
        if message.sender == self.nickname {
            return None;
        }
        if !self.allowed_users.is_empty()
            && !self.allowed_users.contains(&message.sender)
            && !self.allowed_users.contains(&message.prefix)
        {
            return None;
        }
        let direct = message.target.eq_ignore_ascii_case(&self.nickname);
        let chat_id = if direct {
            message.sender.clone()
        } else {
            message.target.clone()
        };
        let chat_type = if direct {
            ChatType::Direct
        } else {
            ChatType::Group
        };
        let route = MessageRoute {
            platform: "irc".to_string(),
            chat_id: chat_id.clone(),
            chat_type,
            user_id: message.sender.clone(),
            display_name: message.sender.clone(),
            thread_id: chat_id.clone(),
        };
        let source_message_id = format!("{}-{}", message.sender, now_millis());
        let metadata = json!({
            "channel": {
                "platform": "irc",
                "adapter": "irc-tcp",
                "chatId": chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": message.sender,
                "sourceMessageId": source_message_id,
                "prefix": message.prefix,
                "target": message.target
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("irc-{source_message_id}"),
            text: message.text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for IrcAdapter {
    fn name(&self) -> &'static str {
        "irc"
    }

    fn capabilities(&self) -> ChannelCapabilityReport {
        ChannelCapabilityReport {
            send: ChannelCapabilityState::Available,
            typing: ChannelCapabilityState::Unavailable,
            edit: ChannelCapabilityState::Unavailable,
            draft: ChannelCapabilityState::Unavailable,
            card: ChannelCapabilityState::Unavailable,
            media: ChannelCapabilityState::Unavailable,
        }
    }

    fn poll_updates(&self) -> ChannelResult<Vec<NormalizedInboundMessage>> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| ChannelError::fatal("irc connection lock poisoned"))?;
        let lines = connection.read_lines()?;
        let mut messages = Vec::new();
        for line in lines {
            if let Some(token) = line.strip_prefix("PING ") {
                connection.write_line(&format!("PONG {token}"))?;
                continue;
            }
            if let Some(message) = self.normalize_line(&line) {
                messages.push(message);
            }
        }
        Ok(messages)
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "irc typing indicator is unavailable",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("irc message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| ChannelError::fatal("irc connection lock poisoned"))?;
        let target = if message.route.chat_type == ChatType::Direct {
            &message.route.user_id
        } else if message.route.chat_id.is_empty() {
            &self.channel
        } else {
            &message.route.chat_id
        };
        for chunk in split_irc_chunks(&message.text) {
            connection.write_line(&format!("PRIVMSG {target} :{chunk}"))?;
        }
        Ok(PlatformSendOutcome {
            message_id: Some(format!("irc-{}", now_millis())),
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "irc {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

struct IrcConnection {
    stream: TcpStream,
    buffer: String,
}

impl IrcConnection {
    fn connect(
        server: &str,
        port: u16,
        nickname: &str,
        channel: &str,
        password: Option<String>,
    ) -> ChannelResult<Self> {
        let address = (server, port)
            .to_socket_addrs()
            .map_err(|error| ChannelError::unavailable(format!("irc resolve failed: {error}")))?
            .next()
            .ok_or_else(|| ChannelError::unavailable("irc server resolved no addresses"))?;
        let stream =
            TcpStream::connect_timeout(&address, Duration::from_millis(IRC_CONNECT_TIMEOUT_MS))
                .map_err(|error| {
                    ChannelError::unavailable(format!("irc connect failed: {error}"))
                })?;
        stream
            .set_read_timeout(Some(Duration::from_millis(IRC_READ_TIMEOUT_MS)))
            .map_err(|error| {
                ChannelError::unavailable(format!("irc read timeout failed: {error}"))
            })?;
        let mut connection = Self {
            stream,
            buffer: String::new(),
        };
        if let Some(password) = password {
            connection.write_line(&format!("PASS {password}"))?;
        }
        connection.write_line(&format!("NICK {nickname}"))?;
        connection.write_line(&format!("USER {nickname} 0 * :flyflor-cli"))?;
        connection.write_line(&format!("JOIN {channel}"))?;
        Ok(connection)
    }

    fn read_lines(&mut self) -> ChannelResult<Vec<String>> {
        let mut bytes = [0_u8; 4096];
        loop {
            match self.stream.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => self
                    .buffer
                    .push_str(&String::from_utf8_lossy(&bytes[..count])),
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    break;
                }
                Err(error) => {
                    return Err(ChannelError::retryable(format!("irc read failed: {error}")));
                }
            }
        }
        Ok(drain_complete_lines(&mut self.buffer))
    }

    fn write_line(&mut self, line: &str) -> ChannelResult<()> {
        self.stream
            .write_all(format!("{line}\r\n").as_bytes())
            .map_err(|error| ChannelError::retryable(format!("irc write failed: {error}")))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IrcPrivmsg {
    prefix: String,
    sender: String,
    target: String,
    text: String,
}

fn parse_privmsg(line: &str) -> Option<IrcPrivmsg> {
    let rest = line.strip_prefix(':')?;
    let (prefix, rest) = rest.split_once(' ')?;
    let (command, rest) = rest.split_once(' ')?;
    if !command.eq_ignore_ascii_case("PRIVMSG") {
        return None;
    }
    let (target, text) = rest.split_once(" :")?;
    let sender = prefix.split('!').next().unwrap_or(prefix).to_string();
    Some(IrcPrivmsg {
        prefix: prefix.to_string(),
        sender,
        target: target.to_string(),
        text: text.trim().to_string(),
    })
}

fn drain_complete_lines(buffer: &mut String) -> Vec<String> {
    let mut lines = Vec::new();
    while let Some(index) = buffer.find('\n') {
        let line = buffer[..index].trim_end_matches(['\r', '\n']).to_string();
        buffer.replace_range(..=index, "");
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

fn split_irc_chunks(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if current.len() + ch.len_utf8() > IRC_MAX_LINE_BYTES {
            chunks.push(current);
            current = String::new();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn outbound_mentions_media(text: &str, metadata: Option<&Value>) -> bool {
    if text.contains("MEDIA:") {
        return true;
    }
    metadata
        .and_then(|value| value.get("media"))
        .is_some_and(|media| !media.is_null())
}

fn first_csv_value(value: &str) -> String {
    value
        .split(',')
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn env_set(name: &str) -> HashSet<String> {
    env::var(name)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_privmsg_and_ignores_other_lines() {
        assert_eq!(
            parse_privmsg(":ada!u@h PRIVMSG #flyflor :hello").unwrap(),
            IrcPrivmsg {
                prefix: "ada!u@h".to_string(),
                sender: "ada".to_string(),
                target: "#flyflor".to_string(),
                text: "hello".to_string(),
            }
        );
        assert!(parse_privmsg("PING :server").is_none());
        assert!(parse_privmsg(":server 001 bot :welcome").is_none());
    }

    #[test]
    fn drain_complete_lines_preserves_partial_tail() {
        let mut buffer = "one\r\ntwo\npar".to_string();
        assert_eq!(drain_complete_lines(&mut buffer), vec!["one", "two"]);
        assert_eq!(buffer, "par");
    }

    #[test]
    fn split_irc_chunks_preserves_unicode_boundaries() {
        assert_eq!(split_irc_chunks("你好世界"), vec!["你好世界"]);
        assert_eq!(
            split_irc_chunks(&"a".repeat(IRC_MAX_LINE_BYTES + 1)).len(),
            2
        );
    }

    #[test]
    fn first_csv_value_uses_first_non_empty_channel() {
        assert_eq!(first_csv_value(" , #one, #two"), "#one");
    }
}
