use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    env, fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Map, Value, json};

use super::platform::{
    ChannelError, ChannelErrorKind, ChannelResult, ChatType, MessageRoute,
    NormalizedInboundMessage, OutboundMessage, PlatformAdapter, PlatformSendOutcome,
};

const ILINK_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
const ILINK_APP_ID: &str = "bot";
const CHANNEL_VERSION: &str = "2.2.0";
const ILINK_APP_CLIENT_VERSION: u32 = (2 << 16) | (2 << 8);

const EP_GET_UPDATES: &str = "ilink/bot/getupdates";
const EP_SEND_MESSAGE: &str = "ilink/bot/sendmessage";
const EP_SEND_TYPING: &str = "ilink/bot/sendtyping";
const EP_GET_CONFIG: &str = "ilink/bot/getconfig";
const EP_GET_BOT_QR: &str = "ilink/bot/get_bot_qrcode";
const EP_GET_QR_STATUS: &str = "ilink/bot/get_qrcode_status";

const LONG_POLL_TIMEOUT_MS: u64 = 35_000;
const API_TIMEOUT_MS: u64 = 15_000;
const CONFIG_TIMEOUT_MS: u64 = 10_000;
const QR_TIMEOUT_MS: u64 = 35_000;

const SESSION_EXPIRED_ERRCODE: i64 = -14;
const RATE_LIMIT_ERRCODE: i64 = -2;
const MESSAGE_DEDUP_TTL_SECONDS: u64 = 300;
const MAX_MESSAGE_LENGTH: usize = 2_000;
const MSG_TYPE_BOT: i64 = 2;
const MSG_STATE_FINISH: i64 = 2;
const ITEM_TEXT: i64 = 1;
const ITEM_IMAGE: i64 = 2;
const ITEM_VOICE: i64 = 3;
const ITEM_FILE: i64 = 4;
const ITEM_VIDEO: i64 = 5;
const TYPING_START: i64 = 1;

#[derive(Clone, Debug)]
pub struct WeixinQrCode {
    pub qrcode: String,
    pub scan_data: String,
    pub qrcode_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WeixinQrStatus {
    Waiting,
    Scanned,
    Redirect { base_url: String },
    Expired,
    Confirmed(WeixinAccount),
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeixinAccount {
    pub account_id: String,
    pub token: String,
    pub base_url: String,
    pub user_id: Option<String>,
}

pub struct WeixinIlinkAdapter {
    account_id: String,
    token: String,
    base_url: String,
    home: PathBuf,
    dm_policy: AccessPolicy,
    group_policy: AccessPolicy,
    allowed_users: HashSet<String>,
    group_allowed_users: HashSet<String>,
    sync_buf: Mutex<String>,
    long_poll_timeout_ms: Mutex<u64>,
    context_tokens: ContextTokenStore,
    typing_tickets: TypingTicketStore,
    dedup: Mutex<TtlDedup>,
    send_retries: usize,
    send_retry_delay: Duration,
}

impl WeixinIlinkAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let home = channel_home();
        let account_id = env::var("WEIXIN_ACCOUNT_ID")
            .or_else(|_| env::var("FLYFLOR_WEIXIN_ACCOUNT_ID"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if account_id.is_empty() {
            return Err(ChannelError::missing_config(
                "WEIXIN_ACCOUNT_ID is required for the weixin channel",
            ));
        }

        let persisted = load_weixin_account(&home, &account_id);
        let token = env::var("WEIXIN_TOKEN")
            .or_else(|_| env::var("FLYFLOR_WEIXIN_TOKEN"))
            .ok()
            .or_else(|| persisted.as_ref().map(|account| account.token.clone()))
            .unwrap_or_default()
            .trim()
            .to_string();
        if token.is_empty() {
            return Err(ChannelError::missing_config(
                "WEIXIN_TOKEN is required unless the account was saved by QR login",
            ));
        }

        let base_url = env::var("WEIXIN_BASE_URL")
            .or_else(|_| env::var("FLYFLOR_WEIXIN_BASE_URL"))
            .ok()
            .or_else(|| persisted.as_ref().map(|account| account.base_url.clone()))
            .unwrap_or_else(|| ILINK_BASE_URL.to_string())
            .trim()
            .trim_end_matches('/')
            .to_string();

        Ok(Self {
            sync_buf: Mutex::new(load_sync_buf(&home, &account_id)),
            long_poll_timeout_ms: Mutex::new(LONG_POLL_TIMEOUT_MS),
            context_tokens: ContextTokenStore::new(home.clone(), account_id.clone()),
            typing_tickets: TypingTicketStore::default(),
            dedup: Mutex::new(TtlDedup::new(
                MESSAGE_DEDUP_TTL_SECONDS,
                env_usize("WEIXIN_DEDUP_MAX", 2_000),
            )),
            account_id,
            token,
            base_url,
            home,
            dm_policy: AccessPolicy::from_env("WEIXIN_DM_POLICY", AccessPolicy::Open),
            group_policy: AccessPolicy::from_env("WEIXIN_GROUP_POLICY", AccessPolicy::Disabled),
            allowed_users: env_set("WEIXIN_ALLOWED_USERS"),
            group_allowed_users: env_set("WEIXIN_GROUP_ALLOWED_USERS"),
            send_retries: env_usize("WEIXIN_SEND_CHUNK_RETRIES", 4),
            send_retry_delay: Duration::from_millis(env_u64(
                "WEIXIN_SEND_CHUNK_RETRY_DELAY_MS",
                1_000,
            )),
        })
    }

    pub fn request_qr_code(&self, bot_type: &str) -> ChannelResult<WeixinQrCode> {
        let response = api_get(
            &self.base_url,
            &format!("{EP_GET_BOT_QR}?bot_type={bot_type}"),
            QR_TIMEOUT_MS,
        )?;
        let qrcode = value_string(&response, "qrcode").ok_or_else(|| {
            ChannelError::fatal("iLink QR response did not include a qrcode field")
        })?;
        let qrcode_url = value_string(&response, "qrcode_img_content");
        Ok(WeixinQrCode {
            scan_data: qrcode_url.clone().unwrap_or_else(|| qrcode.clone()),
            qrcode,
            qrcode_url,
        })
    }

    pub fn poll_qr_status(
        &self,
        qrcode: &str,
        base_url: Option<&str>,
    ) -> ChannelResult<WeixinQrStatus> {
        let response = api_get(
            base_url.unwrap_or(&self.base_url),
            &format!("{EP_GET_QR_STATUS}?qrcode={qrcode}"),
            QR_TIMEOUT_MS,
        )?;
        Ok(match value_string(&response, "status").as_deref() {
            Some("wait") | None => WeixinQrStatus::Waiting,
            Some("scaned") => WeixinQrStatus::Scanned,
            Some("scaned_but_redirect") => {
                let host = value_string(&response, "redirect_host").unwrap_or_default();
                WeixinQrStatus::Redirect {
                    base_url: format!("https://{host}"),
                }
            }
            Some("expired") => WeixinQrStatus::Expired,
            Some("confirmed") => {
                let account = WeixinAccount {
                    account_id: value_string(&response, "ilink_bot_id").unwrap_or_default(),
                    token: value_string(&response, "bot_token").unwrap_or_default(),
                    base_url: value_string(&response, "baseurl")
                        .unwrap_or_else(|| ILINK_BASE_URL.to_string()),
                    user_id: value_string(&response, "ilink_user_id"),
                };
                if account.account_id.is_empty() || account.token.is_empty() {
                    return Err(ChannelError::fatal(
                        "iLink QR confirmation omitted account_id or token",
                    ));
                }
                save_weixin_account(&self.home, &account)?;
                WeixinQrStatus::Confirmed(account)
            }
            Some(other) => WeixinQrStatus::Unknown(other.to_string()),
        })
    }

    fn get_updates(&self) -> ChannelResult<Value> {
        let sync_buf = self
            .sync_buf
            .lock()
            .map(|buf| buf.clone())
            .unwrap_or_default();
        let timeout_ms = self
            .long_poll_timeout_ms
            .lock()
            .map(|timeout| *timeout)
            .unwrap_or(LONG_POLL_TIMEOUT_MS);
        api_post(
            &self.base_url,
            EP_GET_UPDATES,
            json!({ "get_updates_buf": sync_buf }),
            Some(&self.token),
            timeout_ms + 2_000,
        )
    }

    fn normalize_message(&self, message: &Value) -> Option<NormalizedInboundMessage> {
        let sender_id = value_string(message, "from_user_id")?;
        if sender_id.is_empty() || sender_id == self.account_id {
            return None;
        }
        let item_list = message.get("item_list").and_then(Value::as_array)?;
        let message_id = value_string(message, "message_id")
            .unwrap_or_else(|| format!("{}-{}", sender_id, now_millis()));
        if self.is_duplicate(&message_id) {
            return None;
        }

        let (mut text, media_unavailable) = extract_text_and_media_notice(item_list);
        if text.trim().is_empty() && media_unavailable.is_empty() {
            return None;
        }
        if text.trim().is_empty() {
            text = media_unavailable.join("\n");
        } else if !media_unavailable.is_empty() {
            text = format!("{}\n\n{}", text.trim(), media_unavailable.join("\n"));
        }
        let content_key = format!("content:{sender_id}:{}", stable_hash(&text));
        if self.is_duplicate(&content_key) {
            return None;
        }

        let (chat_type, chat_id) = guess_chat_type(message, &self.account_id, &sender_id);
        if !self.is_allowed(&chat_type, &chat_id, &sender_id) {
            return None;
        }

        if let Some(context_token) = value_string(message, "context_token")
            && !context_token.is_empty()
        {
            self.context_tokens.set(&sender_id, &context_token);
            if chat_id != sender_id {
                self.context_tokens.set(&chat_id, &context_token);
            }
        }

        let platform = "weixin".to_string();
        let route = MessageRoute {
            platform: platform.clone(),
            chat_id: chat_id.clone(),
            chat_type,
            user_id: sender_id.clone(),
            display_name: sender_id.clone(),
            thread_id: chat_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": platform,
                "adapter": "weixin-ilink",
                "accountId": self.account_id,
                "chatId": chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": sender_id,
                "sourceMessageId": message_id,
                "contextTokenPresent": message.get("context_token").is_some(),
                "mediaUnavailable": !media_unavailable.is_empty()
            }
        });

        Some(NormalizedInboundMessage {
            id: format!("weixin-{message_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }

    fn is_allowed(&self, chat_type: &ChatType, chat_id: &str, sender_id: &str) -> bool {
        match chat_type {
            ChatType::Direct => match self.dm_policy {
                AccessPolicy::Open => true,
                AccessPolicy::Disabled => false,
                AccessPolicy::Allowlist => self.allowed_users.contains(sender_id),
            },
            ChatType::Group => match self.group_policy {
                AccessPolicy::Open => true,
                AccessPolicy::Disabled => false,
                AccessPolicy::Allowlist => {
                    self.group_allowed_users.contains(chat_id)
                        || self.group_allowed_users.contains(sender_id)
                }
            },
        }
    }

    fn is_duplicate(&self, key: &str) -> bool {
        self.dedup
            .lock()
            .map(|mut dedup| dedup.is_duplicate(key))
            .unwrap_or(false)
    }

    fn send_text_chunk(
        &self,
        chat_id: &str,
        text: &str,
        context_token: Option<String>,
        client_id: &str,
    ) -> ChannelResult<()> {
        let mut context_token = context_token;
        let mut retried_without_token = false;
        let mut last_error = None;

        for attempt in 0..=self.send_retries {
            let response = api_post(
                &self.base_url,
                EP_SEND_MESSAGE,
                build_send_message_payload(chat_id, text, context_token.as_deref(), client_id),
                Some(&self.token),
                API_TIMEOUT_MS,
            );
            match response.and_then(|value| classify_ilink_response(&value).map(|_| value)) {
                Ok(_) => return Ok(()),
                Err(error) if error.kind == ChannelErrorKind::SessionExpired => {
                    if context_token.is_some() && !retried_without_token {
                        retried_without_token = true;
                        context_token = None;
                        self.context_tokens.remove(chat_id);
                        continue;
                    }
                    last_error = Some(error);
                }
                Err(error) if error.kind == ChannelErrorKind::RateLimited => {
                    last_error = Some(error);
                    if attempt < self.send_retries {
                        std::thread::sleep(self.send_retry_delay * 3);
                        continue;
                    }
                }
                Err(error) => {
                    last_error = Some(error);
                }
            }

            if attempt < self.send_retries {
                std::thread::sleep(self.send_retry_delay.saturating_mul((attempt + 1) as u32));
            }
        }

        Err(last_error.unwrap_or_else(|| ChannelError::retryable("iLink sendmessage failed")))
    }

    fn fetch_typing_ticket(&self, route: &MessageRoute) -> ChannelResult<Option<String>> {
        if let Some(ticket) = self.typing_tickets.get(&route.chat_id) {
            return Ok(Some(ticket));
        }
        let mut payload = Map::new();
        payload.insert("ilink_user_id".to_string(), json!(route.chat_id));
        if let Some(context_token) = self.context_tokens.get(&route.chat_id) {
            payload.insert("context_token".to_string(), json!(context_token));
        }
        let response = api_post(
            &self.base_url,
            EP_GET_CONFIG,
            Value::Object(payload),
            Some(&self.token),
            CONFIG_TIMEOUT_MS,
        )?;
        classify_ilink_response(&response)?;
        let ticket = value_string(&response, "typing_ticket");
        if let Some(ticket) = &ticket {
            self.typing_tickets.set(&route.chat_id, ticket);
        }
        Ok(ticket)
    }
}

impl PlatformAdapter for WeixinIlinkAdapter {
    fn name(&self) -> &'static str {
        "weixin"
    }

    fn capabilities(&self) -> super::platform::ChannelCapabilityReport {
        super::platform::ChannelCapabilityReport {
            send: super::platform::ChannelCapabilityState::Available,
            typing: super::platform::ChannelCapabilityState::Available,
            edit: super::platform::ChannelCapabilityState::Unavailable,
            draft: super::platform::ChannelCapabilityState::Unavailable,
            card: super::platform::ChannelCapabilityState::Unavailable,
            media: super::platform::ChannelCapabilityState::Unavailable,
        }
    }

    fn poll_updates(&self) -> ChannelResult<Vec<NormalizedInboundMessage>> {
        let response = self.get_updates()?;
        classify_ilink_response(&response)?;

        if let Some(timeout) = response
            .get("longpolling_timeout_ms")
            .and_then(Value::as_u64)
            .filter(|timeout| *timeout > 0)
            && let Ok(mut current) = self.long_poll_timeout_ms.lock()
        {
            *current = timeout;
        }

        if let Some(new_sync_buf) = value_string(&response, "get_updates_buf")
            && !new_sync_buf.is_empty()
        {
            if let Ok(mut sync_buf) = self.sync_buf.lock() {
                *sync_buf = new_sync_buf.clone();
            }
            let _ = save_sync_buf(&self.home, &self.account_id, &new_sync_buf);
        }

        Ok(response
            .get("msgs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|message| self.normalize_message(message))
            .collect())
    }

    fn send_typing(&self, route: &MessageRoute) -> ChannelResult<()> {
        let Some(ticket) = self.fetch_typing_ticket(route)? else {
            return Ok(());
        };
        let response = api_post(
            &self.base_url,
            EP_SEND_TYPING,
            json!({
                "ilink_user_id": route.chat_id,
                "typing_ticket": ticket,
                "status": TYPING_START
            }),
            Some(&self.token),
            CONFIG_TIMEOUT_MS,
        )?;
        classify_ilink_response(&response)
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "weixin sendmessage text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }

        let context_token = self.context_tokens.get(&message.route.chat_id);
        let chunks = split_text_chunks(&message.text, MAX_MESSAGE_LENGTH);
        let mut last_message_id = None;
        for chunk in chunks {
            let client_id = format!("flyflor-weixin-{}", now_millis());
            self.send_text_chunk(
                &message.route.chat_id,
                &chunk,
                context_token.clone(),
                &client_id,
            )?;
            last_message_id = Some(client_id);
        }
        Ok(PlatformSendOutcome {
            message_id: last_message_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "weixin iLink {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

#[derive(Clone, Copy)]
enum AccessPolicy {
    Open,
    Disabled,
    Allowlist,
}

impl AccessPolicy {
    fn from_env(name: &str, default: Self) -> Self {
        match env::var(name)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "disabled" | "off" | "false" | "0" => Self::Disabled,
            "allowlist" | "allowed" => Self::Allowlist,
            "open" | "all" | "true" | "1" => Self::Open,
            _ => default,
        }
    }
}

struct TtlDedup {
    ttl: Duration,
    max_size: usize,
    seen: HashMap<String, Instant>,
}

impl TtlDedup {
    fn new(ttl_seconds: u64, max_size: usize) -> Self {
        Self {
            ttl: Duration::from_secs(ttl_seconds),
            max_size,
            seen: HashMap::new(),
        }
    }

    fn is_duplicate(&mut self, key: &str) -> bool {
        let now = Instant::now();
        self.seen.retain(|_, at| now.duration_since(*at) < self.ttl);
        if self.seen.contains_key(key) {
            return true;
        }
        self.seen.insert(key.to_string(), now);
        if self.seen.len() > self.max_size {
            let mut entries = self
                .seen
                .iter()
                .map(|(key, at)| (key.clone(), *at))
                .collect::<Vec<_>>();
            entries.sort_by_key(|(_, at)| *at);
            let remove_count = entries.len().saturating_sub(self.max_size);
            for (key, _) in entries.into_iter().take(remove_count) {
                self.seen.remove(&key);
            }
        }
        false
    }
}

struct ContextTokenStore {
    path: PathBuf,
    tokens: Mutex<HashMap<String, String>>,
}

impl ContextTokenStore {
    fn new(home: PathBuf, account_id: String) -> Self {
        let path = account_dir(&home).join(format!("{account_id}.context-tokens.json"));
        let tokens = read_json_map(&path);
        Self {
            path,
            tokens: Mutex::new(tokens),
        }
    }

    fn get(&self, chat_id: &str) -> Option<String> {
        if let Some(token) = self
            .tokens
            .lock()
            .ok()
            .and_then(|tokens| tokens.get(chat_id).cloned())
        {
            return Some(token);
        }
        let restored = read_json_map(&self.path);
        let token = restored.get(chat_id).cloned();
        if let Ok(mut tokens) = self.tokens.lock() {
            *tokens = restored;
        }
        token
    }

    fn set(&self, chat_id: &str, context_token: &str) {
        if context_token.is_empty() {
            return;
        }
        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.insert(chat_id.to_string(), context_token.to_string());
            let _ = write_json_file(&self.path, &json!(*tokens));
        }
    }

    fn remove(&self, chat_id: &str) {
        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.remove(chat_id);
            let _ = write_json_file(&self.path, &json!(*tokens));
        }
    }
}

#[derive(Default)]
struct TypingTicketStore {
    tickets: Mutex<HashMap<String, (String, Instant)>>,
}

impl TypingTicketStore {
    fn get(&self, chat_id: &str) -> Option<String> {
        let mut tickets = self.tickets.lock().ok()?;
        let (ticket, created_at) = tickets.get(chat_id)?;
        if created_at.elapsed() > Duration::from_secs(600) {
            tickets.remove(chat_id);
            return None;
        }
        Some(ticket.clone())
    }

    fn set(&self, chat_id: &str, ticket: &str) {
        if let Ok(mut tickets) = self.tickets.lock() {
            tickets.insert(chat_id.to_string(), (ticket.to_string(), Instant::now()));
        }
    }
}

fn build_send_message_payload(
    chat_id: &str,
    text: &str,
    context_token: Option<&str>,
    client_id: &str,
) -> Value {
    let mut message = json!({
        "from_user_id": "",
        "to_user_id": chat_id,
        "client_id": client_id,
        "message_type": MSG_TYPE_BOT,
        "message_state": MSG_STATE_FINISH,
        "item_list": [{
            "type": ITEM_TEXT,
            "text_item": { "text": text }
        }]
    });
    if let Some(context_token) = context_token.filter(|token| !token.is_empty()) {
        message["context_token"] = json!(context_token);
    }
    json!({ "msg": message })
}

fn api_post(
    base_url: &str,
    endpoint: &str,
    payload: Value,
    token: Option<&str>,
    timeout_ms: u64,
) -> ChannelResult<Value> {
    let mut body = match payload {
        Value::Object(map) => Value::Object(map),
        other => other,
    };
    if let Value::Object(map) = &mut body {
        map.insert(
            "base_info".to_string(),
            json!({ "channel_version": CHANNEL_VERSION }),
        );
    }
    let body =
        serde_json::to_string(&body).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let url = format!("{}/{endpoint}", base_url.trim_end_matches('/'));
    let mut args = vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(timeout_ms),
        "-X".to_string(),
        "POST".to_string(),
        url,
        "-H".to_string(),
        "Content-Type: application/json".to_string(),
        "-H".to_string(),
        "AuthorizationType: ilink_bot_token".to_string(),
        "-H".to_string(),
        format!("Content-Length: {}", body.len()),
        "-H".to_string(),
        format!("X-WECHAT-UIN: {}", random_wechat_uin()),
        "-H".to_string(),
        format!("iLink-App-Id: {ILINK_APP_ID}"),
        "-H".to_string(),
        format!("iLink-App-ClientVersion: {ILINK_APP_CLIENT_VERSION}"),
    ];
    if let Some(token) = token {
        args.extend(["-H".to_string(), format!("Authorization: Bearer {token}")]);
    }
    args.extend(["--data".to_string(), body]);
    run_curl(args)
}

fn api_get(base_url: &str, endpoint: &str, timeout_ms: u64) -> ChannelResult<Value> {
    let url = format!("{}/{endpoint}", base_url.trim_end_matches('/'));
    run_curl(vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(timeout_ms),
        url,
        "-H".to_string(),
        format!("iLink-App-Id: {ILINK_APP_ID}"),
        "-H".to_string(),
        format!("iLink-App-ClientVersion: {ILINK_APP_CLIENT_VERSION}"),
    ])
}

fn run_curl(args: Vec<String>) -> ChannelResult<Value> {
    let output = Command::new("curl")
        .args(args)
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("timed out") || message.contains("Operation timed out") {
            return Ok(json!({ "ret": 0, "msgs": [] }));
        }
        return Err(ChannelError::retryable(format!(
            "curl failed with status {}: {}",
            output.status, message
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|error| {
        ChannelError::retryable(format!(
            "iLink returned invalid JSON: {error}; body={}",
            stdout.trim()
        ))
    })
}

fn classify_ilink_response(value: &Value) -> ChannelResult<()> {
    let ret = value.get("ret").and_then(Value::as_i64);
    let errcode = value.get("errcode").and_then(Value::as_i64);
    let ok_ret = ret.is_none_or(|code| code == 0);
    let ok_err = errcode.is_none_or(|code| code == 0);
    if ok_ret && ok_err {
        return Ok(());
    }
    let errmsg = value_string(value, "errmsg")
        .or_else(|| value_string(value, "msg"))
        .unwrap_or_else(|| "unknown error".to_string());
    if ret == Some(SESSION_EXPIRED_ERRCODE)
        || errcode == Some(SESSION_EXPIRED_ERRCODE)
        || is_stale_session_ret(ret, errcode, &errmsg)
    {
        return Err(ChannelError::session_expired(format!(
            "iLink session expired: ret={ret:?} errcode={errcode:?} errmsg={errmsg}"
        )));
    }
    if ret == Some(RATE_LIMIT_ERRCODE) || errcode == Some(RATE_LIMIT_ERRCODE) {
        return Err(ChannelError::rate_limited(format!(
            "iLink rate limited: ret={ret:?} errcode={errcode:?} errmsg={errmsg}"
        )));
    }
    Err(ChannelError::fatal(format!(
        "iLink error: ret={ret:?} errcode={errcode:?} errmsg={errmsg}"
    )))
}

fn is_stale_session_ret(ret: Option<i64>, errcode: Option<i64>, errmsg: &str) -> bool {
    (ret == Some(RATE_LIMIT_ERRCODE) || errcode == Some(RATE_LIMIT_ERRCODE))
        && errmsg.eq_ignore_ascii_case("unknown error")
}

fn extract_text_and_media_notice(item_list: &[Value]) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut media = Vec::new();
    for item in item_list {
        match item.get("type").and_then(Value::as_i64) {
            Some(ITEM_TEXT) => {
                if let Some(value) = item
                    .get("text_item")
                    .and_then(|text_item| value_string(text_item, "text"))
                {
                    text = value;
                }
                if let Some(ref_item) = item
                    .get("ref_msg")
                    .and_then(|ref_msg| ref_msg.get("message_item"))
                {
                    append_media_notice(ref_item, &mut media);
                }
            }
            Some(ITEM_VOICE) => {
                if let Some(value) = item
                    .get("voice_item")
                    .and_then(|voice_item| value_string(voice_item, "text"))
                {
                    text = value;
                } else {
                    append_media_notice(item, &mut media);
                }
            }
            Some(_) => append_media_notice(item, &mut media),
            None => {}
        }
    }
    (text.trim().to_string(), media)
}

fn append_media_notice(item: &Value, media: &mut Vec<String>) {
    let label = match item.get("type").and_then(Value::as_i64) {
        Some(ITEM_IMAGE) => "image",
        Some(ITEM_VOICE) => "voice",
        Some(ITEM_FILE) => "file",
        Some(ITEM_VIDEO) => "video",
        _ => return,
    };
    media.push(format!(
        "[weixin {label} media unavailable: flyflor-cli does not download iLink media yet]"
    ));
}

fn guess_chat_type(message: &Value, account_id: &str, sender_id: &str) -> (ChatType, String) {
    let room_id = value_string(message, "room_id")
        .or_else(|| value_string(message, "chat_room_id"))
        .unwrap_or_default();
    let to_user_id = value_string(message, "to_user_id").unwrap_or_default();
    let is_group = !room_id.is_empty()
        || (!to_user_id.is_empty()
            && !account_id.is_empty()
            && to_user_id != account_id
            && message.get("msg_type").and_then(Value::as_i64) == Some(1));
    if is_group {
        (
            ChatType::Group,
            first_nonempty(&[room_id, to_user_id, sender_id.to_string()]),
        )
    } else {
        (ChatType::Direct, sender_id.to_string())
    }
}

fn outbound_mentions_media(text: &str, metadata: Option<&Value>) -> bool {
    if text.contains("MEDIA:") {
        return true;
    }
    metadata
        .and_then(|value| value.get("media"))
        .is_some_and(|media| !media.is_null())
}

fn split_text_chunks(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if current.chars().count() >= max_chars {
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

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn first_nonempty(values: &[String]) -> String {
    values
        .iter()
        .find(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

fn stable_hash(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn random_wechat_uin() -> String {
    let raw = format!("{}-{:?}", now_millis(), std::thread::current().id());
    BASE64.encode(raw)
}

fn seconds_arg(timeout_ms: u64) -> String {
    let seconds = (timeout_ms as f64 / 1000.0).max(1.0);
    format!("{seconds:.3}")
}

fn channel_home() -> PathBuf {
    env::var("FLYFLOR_CHANNEL_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".flyflor-cli/gateway"))
}

fn account_dir(home: &Path) -> PathBuf {
    home.join("weixin").join("accounts")
}

fn account_file(home: &Path, account_id: &str) -> PathBuf {
    account_dir(home).join(format!("{account_id}.json"))
}

fn load_weixin_account(home: &Path, account_id: &str) -> Option<WeixinAccount> {
    let value = fs::read_to_string(account_file(home, account_id))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())?;
    Some(WeixinAccount {
        account_id: account_id.to_string(),
        token: value_string(&value, "token")?,
        base_url: value_string(&value, "base_url").unwrap_or_else(|| ILINK_BASE_URL.to_string()),
        user_id: value_string(&value, "user_id"),
    })
}

fn save_weixin_account(home: &Path, account: &WeixinAccount) -> ChannelResult<()> {
    write_json_file(
        &account_file(home, &account.account_id),
        &json!({
            "token": account.token,
            "base_url": account.base_url,
            "user_id": account.user_id,
            "saved_at": now_millis()
        }),
    )
}

fn load_sync_buf(home: &Path, account_id: &str) -> String {
    let path = account_dir(home).join(format!("{account_id}.sync.json"));
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value_string(&value, "get_updates_buf"))
        .unwrap_or_default()
}

fn save_sync_buf(home: &Path, account_id: &str, sync_buf: &str) -> ChannelResult<()> {
    let path = account_dir(home).join(format!("{account_id}.sync.json"));
    write_json_file(&path, &json!({ "get_updates_buf": sync_buf }))
}

fn read_json_map(path: &Path) -> HashMap<String, String> {
    let Ok(text) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) else {
        return HashMap::new();
    };
    map.into_iter()
        .filter_map(|(key, value)| value.as_str().map(|value| (key, value.to_string())))
        .collect()
}

fn write_json_file(path: &Path, value: &Value) -> ChannelResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| ChannelError::fatal(error.to_string()))?;
    }
    let tmp = path.with_extension("tmp");
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| ChannelError::fatal(error.to_string()))?;
    fs::write(&tmp, text).map_err(|error| ChannelError::fatal(error.to_string()))?;
    fs::rename(&tmp, path).map_err(|error| ChannelError::fatal(error.to_string()))?;
    set_private_permissions(path);
    Ok(())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) {}

fn env_set(name: &str) -> HashSet<String> {
    env::var(name)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sendmessage_payload_echoes_context_token() {
        let payload = build_send_message_payload("user-1", "hello", Some("ctx-1"), "client-1");
        let message = payload.get("msg").unwrap();

        assert_eq!(
            message.get("to_user_id").and_then(Value::as_str),
            Some("user-1")
        );
        assert_eq!(
            message.get("context_token").and_then(Value::as_str),
            Some("ctx-1")
        );
        assert_eq!(
            message
                .get("item_list")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("text_item"))
                .and_then(|item| item.get("text"))
                .and_then(Value::as_str),
            Some("hello")
        );
    }

    #[test]
    fn classifies_session_expiry_and_rate_limit() {
        assert_eq!(
            classify_ilink_response(&json!({ "errcode": -14, "errmsg": "expired" }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_ilink_response(&json!({ "ret": -2, "errmsg": "busy" }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_ilink_response(&json!({ "ret": -2, "errmsg": "unknown error" }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::SessionExpired
        );
    }

    #[test]
    fn normalizes_text_media_notice_and_context_token() {
        let home = PathBuf::from(format!("/tmp/flyflor-weixin-test-{}", now_millis()));
        let adapter = WeixinIlinkAdapter {
            account_id: "bot".to_string(),
            token: "token".to_string(),
            base_url: ILINK_BASE_URL.to_string(),
            home: home.clone(),
            dm_policy: AccessPolicy::Open,
            group_policy: AccessPolicy::Disabled,
            allowed_users: HashSet::new(),
            group_allowed_users: HashSet::new(),
            sync_buf: Mutex::new(String::new()),
            long_poll_timeout_ms: Mutex::new(LONG_POLL_TIMEOUT_MS),
            context_tokens: ContextTokenStore::new(home.clone(), "bot".to_string()),
            typing_tickets: TypingTicketStore::default(),
            dedup: Mutex::new(TtlDedup::new(300, 100)),
            send_retries: 0,
            send_retry_delay: Duration::from_millis(1),
        };

        let normalized = adapter
            .normalize_message(&json!({
                "from_user_id": "user-1",
                "to_user_id": "bot",
                "message_id": "m-1",
                "context_token": "ctx-1",
                "item_list": [
                    { "type": 1, "text_item": { "text": "hi" } },
                    { "type": 2, "image_item": { "media": { "full_url": "https://example.invalid/a.jpg" } } }
                ]
            }))
            .unwrap();

        assert_eq!(normalized.id, "weixin-m-1");
        assert!(normalized.text.contains("hi"));
        assert!(normalized.text.contains("media unavailable"));
        assert_eq!(normalized.route.chat_type, ChatType::Direct);
        assert_eq!(
            adapter.context_tokens.get("user-1"),
            Some("ctx-1".to_string())
        );
        assert!(
            adapter
                .normalize_message(&json!({
                    "from_user_id": "user-1",
                    "message_id": "m-1",
                    "item_list": [{ "type": 1, "text_item": { "text": "hi" } }]
                }))
                .is_none()
        );

        let _ = fs::remove_dir_all(home);
    }
}
