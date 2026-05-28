use std::{collections::HashMap, env, sync::Arc};

use serde_json::{Value, json};

use crate::tui::gateway::platforms::all_platforms;

use super::bluebubbles::BlueBubblesAdapter;
use super::dingtalk::DingTalkAdapter;
use super::discord::DiscordAdapter;
use super::email::EmailAdapter;
use super::feishu::FeishuAdapter;
use super::google_chat::GoogleChatAdapter;
use super::homeassistant::HomeAssistantAdapter;
use super::irc::IrcAdapter;
use super::line::LineAdapter;
use super::matrix::MatrixAdapter;
use super::mattermost::MattermostAdapter;
use super::msgraph_webhook::MsGraphWebhookAdapter;
use super::ntfy::NtfyAdapter;
use super::openwebui::OpenWebuiAdapter;
use super::qqbot::QqBotAdapter;
use super::signal::SignalAdapter;
use super::simplex::SimplexAdapter;
use super::slack::SlackAdapter;
use super::sms::SmsAdapter;
use super::teams::TeamsAdapter;
use super::telegram::TelegramBotAdapter;
use super::webhook::WebhookAdapter;
use super::wecom::WeComAdapter;
use super::wecom_callback::WeComCallbackAdapter;
use super::weixin::WeixinIlinkAdapter;
use super::whatsapp::WhatsAppAdapter;
use super::yuanbao::YuanbaoAdapter;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelErrorKind {
    Unavailable,
    MissingConfig,
    SessionExpired,
    RateLimited,
    Retryable,
    Fatal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelError {
    pub kind: ChannelErrorKind,
    pub message: String,
}

impl ChannelError {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            kind: ChannelErrorKind::Unavailable,
            message: message.into(),
        }
    }

    pub fn missing_config(message: impl Into<String>) -> Self {
        Self {
            kind: ChannelErrorKind::MissingConfig,
            message: message.into(),
        }
    }

    pub fn session_expired(message: impl Into<String>) -> Self {
        Self {
            kind: ChannelErrorKind::SessionExpired,
            message: message.into(),
        }
    }

    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self {
            kind: ChannelErrorKind::RateLimited,
            message: message.into(),
        }
    }

    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            kind: ChannelErrorKind::Retryable,
            message: message.into(),
        }
    }

    pub fn fatal(message: impl Into<String>) -> Self {
        Self {
            kind: ChannelErrorKind::Fatal,
            message: message.into(),
        }
    }
}

pub type ChannelResult<T> = Result<T, ChannelError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatType {
    Direct,
    Group,
}

impl ChatType {
    pub fn as_gateway_str(&self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Group => "group",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRoute {
    pub platform: String,
    pub chat_id: String,
    pub chat_type: ChatType,
    pub user_id: String,
    pub display_name: String,
    pub thread_id: String,
}

#[derive(Clone, Debug)]
pub struct NormalizedInboundMessage {
    pub id: String,
    pub text: String,
    pub route: MessageRoute,
    pub context: Option<Value>,
    pub metadata: Value,
}

#[derive(Clone, Debug)]
pub struct OutboundMessage {
    pub route: MessageRoute,
    pub text: String,
    pub reply_to_message_id: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct OutboundStreamUpdate {
    pub route: MessageRoute,
    pub message_id: String,
    pub text: String,
    pub mode: StreamDeliveryMode,
    pub final_update: bool,
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamDeliveryMode {
    Edit,
    Draft,
    Card,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformSendOutcome {
    pub message_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelCapabilityState {
    Available,
    Degraded,
    Unavailable,
}

impl ChannelCapabilityState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelCapabilityReport {
    pub send: ChannelCapabilityState,
    pub typing: ChannelCapabilityState,
    pub edit: ChannelCapabilityState,
    pub draft: ChannelCapabilityState,
    pub card: ChannelCapabilityState,
    pub media: ChannelCapabilityState,
}

impl ChannelCapabilityReport {
    pub fn send_only() -> Self {
        Self {
            send: ChannelCapabilityState::Available,
            typing: ChannelCapabilityState::Degraded,
            edit: ChannelCapabilityState::Unavailable,
            draft: ChannelCapabilityState::Unavailable,
            card: ChannelCapabilityState::Unavailable,
            media: ChannelCapabilityState::Unavailable,
        }
    }

    pub fn unavailable() -> Self {
        Self {
            send: ChannelCapabilityState::Unavailable,
            typing: ChannelCapabilityState::Unavailable,
            edit: ChannelCapabilityState::Unavailable,
            draft: ChannelCapabilityState::Unavailable,
            card: ChannelCapabilityState::Unavailable,
            media: ChannelCapabilityState::Unavailable,
        }
    }

    pub fn supports_stream_mode(&self) -> Option<StreamDeliveryMode> {
        if self.card == ChannelCapabilityState::Available {
            return Some(StreamDeliveryMode::Card);
        }
        if self.draft == ChannelCapabilityState::Available {
            return Some(StreamDeliveryMode::Draft);
        }
        if self.edit == ChannelCapabilityState::Available {
            return Some(StreamDeliveryMode::Edit);
        }
        None
    }

    pub fn as_metadata(&self) -> Value {
        json!({
            "send": self.send.as_str(),
            "typing": self.typing.as_str(),
            "edit": self.edit.as_str(),
            "draft": self.draft.as_str(),
            "card": self.card.as_str(),
            "media": self.media.as_str()
        })
    }
}

pub trait PlatformAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> ChannelCapabilityReport {
        ChannelCapabilityReport::send_only()
    }
    fn poll_updates(&self) -> ChannelResult<Vec<NormalizedInboundMessage>>;
    fn send_typing(&self, route: &MessageRoute) -> ChannelResult<()>;
    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome>;

    fn stream_update(&self, update: OutboundStreamUpdate) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "{:?} streaming update is unavailable for {} message {}",
            update.mode,
            self.name(),
            update.message_id
        )))
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "{} media delivery is unavailable for {}",
            media_kind,
            self.name()
        )))
    }
}

pub struct PlatformEntry {
    pub name: &'static str,
    pub label: &'static str,
    pub factory: Box<dyn Fn() -> ChannelResult<Arc<dyn PlatformAdapter>> + Send + Sync>,
    pub native_runtime: bool,
}

#[derive(Default)]
pub struct PlatformRegistry {
    entries: HashMap<&'static str, PlatformEntry>,
}

impl PlatformRegistry {
    pub fn with_builtin_platforms() -> Self {
        let mut registry = Self::default();
        for platform in all_platforms() {
            let name = platform.name;
            let label = platform.label;
            if name == "telegram" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        TelegramBotAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "discord" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        DiscordAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "slack" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        SlackAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "ntfy" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        NtfyAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "matrix" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        MatrixAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "whatsapp" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        WhatsAppAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "feishu" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        FeishuAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "google-chat" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        GoogleChatAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "msgraph-webhook" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        MsGraphWebhookAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "dingtalk" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        DingTalkAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "wecom" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        WeComAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "qqbot" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        QqBotAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "signal" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        SignalAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "simplex" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        SimplexAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "wecom-callback" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        WeComCallbackAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "irc" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        IrcAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "mattermost" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        MattermostAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "email" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        EmailAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "homeassistant" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        HomeAssistantAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "open-webui" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        OpenWebuiAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "sms" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        SmsAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "bluebubbles" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        BlueBubblesAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "line" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        LineAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "weixin" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        WeixinIlinkAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "webhook" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        WebhookAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "teams" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        TeamsAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            if name == "yuanbao" {
                registry.register(PlatformEntry {
                    name,
                    label,
                    factory: Box::new(|| {
                        YuanbaoAdapter::from_env().map(|adapter| Arc::new(adapter) as _)
                    }),
                    native_runtime: true,
                });
                continue;
            }
            registry.register(PlatformEntry {
                name,
                label,
                factory: Box::new(move || {
                    Ok(Arc::new(UnsupportedPlatformAdapter { name, label }) as _)
                }),
                native_runtime: false,
            });
        }
        registry
    }

    pub fn register(&mut self, entry: PlatformEntry) {
        self.entries.insert(entry.name, entry);
    }

    pub fn get(&self, name: &str) -> Option<&PlatformEntry> {
        self.entries.get(name)
    }

    pub fn names(&self) -> Vec<&'static str> {
        let mut names = self.entries.keys().copied().collect::<Vec<_>>();
        names.sort_unstable();
        names
    }
}

pub fn enabled_platform_names_from_env() -> Vec<String> {
    env::var("FLYFLOR_GATEWAY_CHANNELS")
        .or_else(|_| env::var("FLYFLOR_CHANNELS"))
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(|name| name.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_else(|_| {
            crate::tui::gateway::config::enabled_channel_names_from_default_config()
        })
}

struct UnsupportedPlatformAdapter {
    name: &'static str,
    label: &'static str,
}

impl PlatformAdapter for UnsupportedPlatformAdapter {
    fn name(&self) -> &'static str {
        self.name
    }

    fn capabilities(&self) -> ChannelCapabilityReport {
        ChannelCapabilityReport::unavailable()
    }

    fn poll_updates(&self) -> ChannelResult<Vec<NormalizedInboundMessage>> {
        Err(ChannelError::unavailable(format!(
            "{} channel is registered but not implemented in flyflor-cli",
            self.label
        )))
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(format!(
            "{} sendtyping is not implemented",
            self.label
        )))
    }

    fn send_message(&self, _message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "{} sendmessage is not implemented",
            self.label
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_advertises_native_platforms_without_fake_success() {
        let registry = PlatformRegistry::with_builtin_platforms();

        assert!(
            registry
                .get("weixin")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("telegram")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("discord")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("slack")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("webhook")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("ntfy")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("matrix")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("whatsapp")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("feishu")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("google-chat")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("msgraph-webhook")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("dingtalk")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("qqbot")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("wecom")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("signal")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("simplex")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("wecom-callback")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("irc")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("mattermost")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("bluebubbles")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("email")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("teams")
                .is_some_and(|entry| entry.native_runtime)
        );
        assert!(
            registry
                .get("yuanbao")
                .is_some_and(|entry| entry.native_runtime)
        );

        let teams_result = (registry.get("teams").unwrap().factory)();
        assert!(matches!(
            teams_result,
            Err(ChannelError {
                kind: ChannelErrorKind::MissingConfig,
                ..
            })
        ));

        let yuanbao_result = (registry.get("yuanbao").unwrap().factory)();
        assert!(matches!(
            yuanbao_result,
            Err(ChannelError {
                kind: ChannelErrorKind::MissingConfig,
                ..
            })
        ));

        assert!(registry.names().into_iter().all(|name| {
            registry
                .get(name)
                .is_some_and(|entry| entry.native_runtime)
        }));
    }
}
