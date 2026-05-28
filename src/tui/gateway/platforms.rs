#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformRuntimeStatus {
    Native,
    Planned,
}

impl PlatformRuntimeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Planned => "planned",
        }
    }

    pub fn native_runtime(&self) -> bool {
        matches!(self, Self::Native)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformCapability {
    pub inbound_text: bool,
    pub outbound_text: bool,
    pub inbound_media: bool,
    pub outbound_media: bool,
    pub typing: bool,
    pub reactions: bool,
    pub read_receipts: bool,
    pub message_edit: bool,
    pub stream_update: bool,
    pub cards: bool,
    pub approval_buttons: bool,
    pub slash_commands: bool,
    pub threads: bool,
    pub group_chat: bool,
    pub direct_message: bool,
    pub quote_reply: bool,
    pub undo: bool,
    pub file_download: bool,
    pub file_upload: bool,
    pub voice: bool,
    pub webhook_required: bool,
    pub long_poll: bool,
    pub websocket: bool,
    pub polling: bool,
    pub oauth: bool,
    pub qr_login: bool,
    pub service_install: bool,
}

impl PlatformCapability {
    pub const fn text() -> Self {
        Self {
            inbound_text: true,
            outbound_text: true,
            inbound_media: false,
            outbound_media: false,
            typing: false,
            reactions: false,
            read_receipts: false,
            message_edit: false,
            stream_update: false,
            cards: false,
            approval_buttons: false,
            slash_commands: false,
            threads: false,
            group_chat: false,
            direct_message: true,
            quote_reply: false,
            undo: false,
            file_download: false,
            file_upload: false,
            voice: false,
            webhook_required: false,
            long_poll: false,
            websocket: false,
            polling: false,
            oauth: false,
            qr_login: false,
            service_install: false,
        }
    }

    pub fn feature_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.inbound_text {
            names.push("inbound-text");
        }
        if self.outbound_text {
            names.push("outbound-text");
        }
        if self.inbound_media {
            names.push("inbound-media");
        }
        if self.outbound_media {
            names.push("outbound-media");
        }
        if self.typing {
            names.push("typing");
        }
        if self.reactions {
            names.push("reactions");
        }
        if self.read_receipts {
            names.push("read-receipts");
        }
        if self.message_edit {
            names.push("message-edit");
        }
        if self.stream_update {
            names.push("stream-update");
        }
        if self.cards {
            names.push("cards");
        }
        if self.approval_buttons {
            names.push("approval-buttons");
        }
        if self.slash_commands {
            names.push("slash-commands");
        }
        if self.threads {
            names.push("threads");
        }
        if self.group_chat {
            names.push("group-chat");
        }
        if self.direct_message {
            names.push("dm");
        }
        if self.quote_reply {
            names.push("quote-reply");
        }
        if self.undo {
            names.push("undo");
        }
        if self.file_download {
            names.push("file-download");
        }
        if self.file_upload {
            names.push("file-upload");
        }
        if self.voice {
            names.push("voice");
        }
        if self.webhook_required {
            names.push("webhook");
        }
        if self.long_poll {
            names.push("long-poll");
        }
        if self.websocket {
            names.push("websocket");
        }
        if self.polling {
            names.push("polling");
        }
        if self.oauth {
            names.push("oauth");
        }
        if self.qr_login {
            names.push("qr-login");
        }
        if self.service_install {
            names.push("service-install");
        }
        names
    }
}

macro_rules! cap {
    ($($field:ident),* $(,)?) => {{
        let mut capability = PlatformCapability::text();
        $(capability.$field = true;)*
        capability
    }};
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformMetadata {
    pub name: &'static str,
    pub label: &'static str,
    pub source_channel: &'static str,
    pub aliases: &'static [&'static str],
    pub required_env: &'static [&'static str],
    pub optional_env: &'static [&'static str],
    pub env_aliases: &'static [&'static str],
    pub status: PlatformRuntimeStatus,
    pub capability: PlatformCapability,
    pub details: &'static [&'static str],
}

impl PlatformMetadata {
    pub fn native_runtime(&self) -> bool {
        self.status.native_runtime()
    }
}

pub const PLATFORMS: &[PlatformMetadata] = &[
    PlatformMetadata {
        name: "telegram",
        label: "Telegram",
        source_channel: "telegram",
        aliases: &[],
        required_env: &["TELEGRAM_BOT_TOKEN"],
        optional_env: &[
            "TELEGRAM_ALLOWED_USERS",
            "TELEGRAM_ALLOW_ALL_USERS",
            "TELEGRAM_WEBHOOK_SECRET",
            "TELEGRAM_HOME_CHAT_ID",
        ],
        env_aliases: &[
            "TELEGRAM_BOT_TOKEN",
            "TELEGRAM_TOKEN",
            "TELEGRAM_CHAT_ID",
            "FLYFLOR_TELEGRAM_BOT_TOKEN",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            reactions,
            message_edit,
            stream_update,
            cards,
            approval_buttons,
            slash_commands,
            threads,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            voice,
            long_poll,
            webhook_required
        ),
        details: &[
            "long-poll/webhook mode",
            "forum topics",
            "inline approvals",
            "slash commands",
            "document/photo/audio handling",
        ],
    },
    PlatformMetadata {
        name: "discord",
        label: "Discord",
        source_channel: "discord",
        aliases: &[],
        required_env: &["DISCORD_BOT_TOKEN", "DISCORD_HOME_CHANNEL"],
        optional_env: &[
            "DISCORD_ALLOWED_USERS",
            "DISCORD_BOT_USER_ID",
            "DISCORD_SINCE_MESSAGE_ID",
            "DISCORD_API_BASE",
        ],
        env_aliases: &[
            "DISCORD_BOT_TOKEN",
            "DISCORD_TOKEN",
            "DISCORD_HOME_CHANNEL",
            "FLYFLOR_DISCORD_TOKEN",
            "FLYFLOR_DISCORD_HOME_CHANNEL",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(group_chat, polling),
        details: &[
            "channel messages REST polling",
            "create message REST replies",
            "allowed mentions",
            "bot/self filters",
            "media/components/slash commands unavailable",
        ],
    },
    PlatformMetadata {
        name: "slack",
        label: "Slack",
        source_channel: "slack",
        aliases: &[],
        required_env: &["SLACK_BOT_TOKEN", "SLACK_HOME_CHANNEL"],
        optional_env: &[
            "SLACK_ALLOWED_USERS",
            "SLACK_API_BASE",
            "SLACK_BOT_USER_ID",
            "SLACK_REPLY_IN_THREAD",
            "SLACK_SINCE_TS",
        ],
        env_aliases: &[
            "SLACK_BOT_TOKEN",
            "SLACK_HOME_CHANNEL",
            "FLYFLOR_SLACK_BOT_TOKEN",
            "FLYFLOR_SLACK_HOME_CHANNEL",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(threads, group_chat, polling),
        details: &[
            "conversations.history REST polling",
            "chat.postMessage replies",
            "thread_ts preservation",
            "bot/self filters",
            "Socket Mode/blocks/files unavailable",
        ],
    },
    PlatformMetadata {
        name: "matrix",
        label: "Matrix",
        source_channel: "matrix",
        aliases: &[],
        required_env: &["MATRIX_HOMESERVER", "MATRIX_ACCESS_TOKEN", "MATRIX_USER_ID"],
        optional_env: &[
            "MATRIX_ALLOWED_USERS",
            "MATRIX_REQUIRE_MENTION",
            "MATRIX_AUTO_THREAD",
        ],
        env_aliases: &[
            "MATRIX_HOMESERVER",
            "MATRIX_ACCESS_TOKEN",
            "MATRIX_USER_ID",
            "FLYFLOR_MATRIX_ACCESS_TOKEN",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(typing, group_chat, polling),
        details: &[
            "client-server sync polling",
            "room routing",
            "plain text messages",
            "typing indicator",
            "E2EE/media/reactions explicit unavailable",
        ],
    },
    PlatformMetadata {
        name: "whatsapp",
        label: "WhatsApp",
        source_channel: "whatsapp",
        aliases: &[],
        required_env: &["WHATSAPP_ACCESS_TOKEN", "WHATSAPP_PHONE_NUMBER_ID"],
        optional_env: &[
            "WHATSAPP_ALLOWED_USERS",
            "WHATSAPP_API_VERSION",
            "WHATSAPP_BUSINESS_ACCOUNT_ID",
            "WHATSAPP_GRAPH_BASE",
            "WHATSAPP_INBOUND_WEBHOOK",
        ],
        env_aliases: &[
            "WHATSAPP_ACCESS_TOKEN",
            "WHATSAPP_PHONE_NUMBER_ID",
            "FLYFLOR_WHATSAPP_TOKEN",
            "FLYFLOR_WHATSAPP_PHONE_NUMBER_ID",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: PlatformCapability::text(),
        details: &[
            "Cloud API webhook payload",
            "Graph messages text send",
            "phone allowlist",
            "direct text routing",
            "Baileys/QR/media unavailable",
        ],
    },
    PlatformMetadata {
        name: "feishu",
        label: "Feishu/Lark",
        source_channel: "feishu",
        aliases: &["lark", "feishu-lark"],
        required_env: &["FEISHU_APP_ID", "FEISHU_APP_SECRET"],
        optional_env: &[
            "FEISHU_VERIFICATION_TOKEN",
            "FEISHU_ENCRYPT_KEY",
            "FEISHU_ALLOWED_USERS",
            "LARK_APP_ID",
            "LARK_APP_SECRET",
        ],
        env_aliases: &[
            "FEISHU_APP_ID",
            "FEISHU_APP_SECRET",
            "LARK_APP_ID",
            "LARK_APP_SECRET",
            "FLYFLOR_FEISHU_APP_SECRET",
        ],
        status: PlatformRuntimeStatus::Planned,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            message_edit,
            stream_update,
            cards,
            approval_buttons,
            slash_commands,
            threads,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            webhook_required
        ),
        details: &[
            "interactive card approvals",
            "card streaming updates",
            "bot admission",
            "ACL",
            "doc/drive tools remain separate",
        ],
    },
    PlatformMetadata {
        name: "dingtalk",
        label: "DingTalk",
        source_channel: "dingtalk",
        aliases: &["ding-talk"],
        required_env: &["DINGTALK_CLIENT_ID", "DINGTALK_CLIENT_SECRET"],
        optional_env: &[
            "DINGTALK_APP_KEY",
            "DINGTALK_APP_SECRET",
            "DINGTALK_ROBOT_CODE",
            "DINGTALK_ALLOWED_USERS",
        ],
        env_aliases: &[
            "DINGTALK_CLIENT_ID",
            "DINGTALK_CLIENT_SECRET",
            "DINGTALK_APP_KEY",
            "DINGTALK_APP_SECRET",
            "FLYFLOR_DINGTALK_APP_SECRET",
        ],
        status: PlatformRuntimeStatus::Planned,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            reactions,
            message_edit,
            stream_update,
            cards,
            approval_buttons,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            websocket,
            qr_login,
            oauth
        ),
        details: &[
            "Stream Mode",
            "QR device flow",
            "AI cards",
            "session webhook replies",
            "media OpenAPI",
        ],
    },
    PlatformMetadata {
        name: "wecom",
        label: "WeCom",
        source_channel: "wecom",
        aliases: &["wechat-work"],
        required_env: &["WECOM_BOT_ID", "WECOM_SECRET"],
        optional_env: &[
            "WECOM_ALLOWED_USERS",
            "WECOM_HOME_CHANNEL",
            "WECOM_WEBSOCKET_URL",
        ],
        env_aliases: &[
            "WECOM_BOT_ID",
            "WECOM_SECRET",
            "WECOM_CORP_ID",
            "WECOM_AGENT_ID",
            "FLYFLOR_WECOM_SECRET",
        ],
        status: PlatformRuntimeStatus::Planned,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            message_edit,
            stream_update,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            voice,
            websocket,
            qr_login
        ),
        details: &[
            "AI Bot WebSocket",
            "scan-to-create",
            "AES media",
            "reply-mode streaming",
            "per-group policies",
        ],
    },
    PlatformMetadata {
        name: "wecom-callback",
        label: "WeCom Callback",
        source_channel: "wecom-callback",
        aliases: &["wecom_callback"],
        required_env: &[
            "WECOM_CALLBACK_TOKEN",
            "WECOM_CALLBACK_AES_KEY",
            "WECOM_CORP_ID",
            "WECOM_CORP_SECRET",
            "WECOM_AGENT_ID",
        ],
        optional_env: &[
            "WECOM_CALLBACK_HOST",
            "WECOM_CALLBACK_PORT",
            "WECOM_CALLBACK_ALLOWED_USERS",
        ],
        env_aliases: &[
            "WECOM_CALLBACK_TOKEN",
            "WECOM_CALLBACK_AES_KEY",
            "WECOM_CORP_ID",
            "WECOM_CORP_SECRET",
            "FLYFLOR_WECOM_CALLBACK_TOKEN",
        ],
        status: PlatformRuntimeStatus::Planned,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            message_edit,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            webhook_required
        ),
        details: &[
            "encrypted callback verification",
            "multi-app routing",
            "corp scoped users",
            "access-token cache",
        ],
    },
    PlatformMetadata {
        name: "weixin",
        label: "Weixin iLink",
        source_channel: "weixin",
        aliases: &["weixin-ilink", "wechat", "ilink"],
        required_env: &["WEIXIN_ACCOUNT_ID"],
        optional_env: &[
            "WEIXIN_TOKEN",
            "WEIXIN_BASE_URL",
            "WEIXIN_DM_POLICY",
            "WEIXIN_GROUP_POLICY",
            "WEIXIN_ALLOWED_USERS",
            "WEIXIN_HOME_CHANNEL",
        ],
        env_aliases: &[
            "WEIXIN_ACCOUNT_ID",
            "WEIXIN_TOKEN",
            "WEIXIN_BASE_URL",
            "WEIXIN_DM_POLICY",
            "WEIXIN_GROUP_POLICY",
            "FLYFLOR_WEIXIN_ACCOUNT_ID",
            "FLYFLOR_WEIXIN_TOKEN",
            "FLYFLOR_WEIXIN_BASE_URL",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            voice,
            long_poll,
            qr_login
        ),
        details: &[
            "iLink Bot API",
            "QR login helpers",
            "context token persistence",
            "AES media hooks",
            "dedup/retry/rate-limit classification",
        ],
    },
    PlatformMetadata {
        name: "qqbot",
        label: "QQBot",
        source_channel: "qqbot",
        aliases: &["qq"],
        required_env: &["QQBOT_APP_ID", "QQBOT_SECRET"],
        optional_env: &["QQBOT_TOKEN", "QQBOT_ALLOWED_USERS", "QQBOT_HOME_CHANNEL"],
        env_aliases: &[
            "QQBOT_APP_ID",
            "QQBOT_SECRET",
            "QQBOT_TOKEN",
            "FLYFLOR_QQBOT_TOKEN",
        ],
        status: PlatformRuntimeStatus::Planned,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            websocket
        ),
        details: &[
            "direct/group support",
            "mention gating",
            "rate-limit classification",
            "signature verification",
        ],
    },
    PlatformMetadata {
        name: "email",
        label: "Email",
        source_channel: "email",
        aliases: &["smtp", "imap"],
        required_env: &["EMAIL_ADDRESS", "EMAIL_PASSWORD", "EMAIL_SMTP_HOST"],
        optional_env: &[
            "EMAIL_INBOUND_MESSAGE",
            "EMAIL_ALLOWED_USERS",
            "EMAIL_SMTP_PORT",
            "EMAIL_HOME_ADDRESS",
        ],
        env_aliases: &[
            "EMAIL_ADDRESS",
            "EMAIL_PASSWORD",
            "EMAIL_SMTP_HOST",
            "FLYFLOR_EMAIL_ADDRESS",
            "FLYFLOR_EMAIL_PASSWORD",
            "FLYFLOR_EMAIL_SMTP_HOST",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(direct_message, polling),
        details: &[
            "env inbound payload",
            "plain SMTP text replies",
            "attachments unavailable",
            "IMAP/TLS follow-up",
            "noreply/self-loop filters",
        ],
    },
    PlatformMetadata {
        name: "webhook",
        label: "Webhook",
        source_channel: "webhook",
        aliases: &[],
        required_env: &["WEBHOOK_SECRET"],
        optional_env: &[
            "WEBHOOK_BIND",
            "WEBHOOK_PUBLIC_URL",
            "WEBHOOK_ALLOWED_SOURCES",
        ],
        env_aliases: &[
            "WEBHOOK_SECRET",
            "WEBHOOK_BIND",
            "WEBHOOK_PUBLIC_URL",
            "FLYFLOR_WEBHOOK_SECRET",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(
            inbound_media,
            outbound_media,
            message_edit,
            stream_update,
            cards,
            quote_reply,
            file_download,
            file_upload,
            webhook_required
        ),
        details: &[
            "dynamic routes",
            "signature verification",
            "rate limiting",
            "delivery callbacks",
        ],
    },
    PlatformMetadata {
        name: "teams",
        label: "Microsoft Teams",
        source_channel: "teams",
        aliases: &["microsoft-teams", "ms-teams"],
        required_env: &["TEAMS_CLIENT_ID", "TEAMS_CLIENT_SECRET", "TEAMS_TENANT_ID"],
        optional_env: &[
            "TEAMS_PORT",
            "TEAMS_ALLOWED_USERS",
            "TEAMS_ALLOW_ALL_USERS",
            "TEAMS_HOME_CHANNEL",
        ],
        env_aliases: &[
            "TEAMS_TENANT_ID",
            "TEAMS_CLIENT_ID",
            "TEAMS_CLIENT_SECRET",
            "TEAMS_WEBHOOK_URL",
            "FLYFLOR_TEAMS_CLIENT_SECRET",
        ],
        status: PlatformRuntimeStatus::Planned,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            message_edit,
            stream_update,
            cards,
            approval_buttons,
            threads,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            webhook_required
        ),
        details: &[
            "Bot Framework",
            "Adaptive Card approvals",
            "proactive messaging",
            "channel posts",
        ],
    },
    PlatformMetadata {
        name: "msgraph-webhook",
        label: "Microsoft Graph Webhook",
        source_channel: "msgraph-webhook",
        aliases: &["msgraph", "microsoft-graph"],
        required_env: &[
            "MSGRAPH_TENANT_ID",
            "MSGRAPH_CLIENT_ID",
            "MSGRAPH_CLIENT_SECRET",
        ],
        optional_env: &["MSGRAPH_WEBHOOK_SECRET", "MSGRAPH_HOME_CHANNEL"],
        env_aliases: &[
            "MSGRAPH_TENANT_ID",
            "MSGRAPH_CLIENT_ID",
            "MSGRAPH_CLIENT_SECRET",
            "MSGRAPH_WEBHOOK_SECRET",
            "FLYFLOR_MSGRAPH_CLIENT_SECRET",
        ],
        status: PlatformRuntimeStatus::Planned,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            message_edit,
            stream_update,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            webhook_required,
            oauth
        ),
        details: &[
            "Graph webhook validation",
            "pipeline runtime",
            "tenant/app scoping",
            "outbound delivery",
        ],
    },
    PlatformMetadata {
        name: "google-chat",
        label: "Google Chat",
        source_channel: "google-chat",
        aliases: &["gchat", "google_chat"],
        required_env: &[
            "GOOGLE_CHAT_PROJECT_ID",
            "GOOGLE_CHAT_SUBSCRIPTION_NAME",
            "GOOGLE_CHAT_SERVICE_ACCOUNT_JSON",
        ],
        optional_env: &[
            "GOOGLE_CHAT_ALLOWED_USERS",
            "GOOGLE_CHAT_HOME_CHANNEL",
            "GOOGLE_APPLICATION_CREDENTIALS",
        ],
        env_aliases: &[
            "GOOGLE_CHAT_PROJECT_ID",
            "GOOGLE_CHAT_SUBSCRIPTION_NAME",
            "GOOGLE_CHAT_SERVICE_ACCOUNT_JSON",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "FLYFLOR_GOOGLE_CHAT_SERVICE_ACCOUNT_JSON",
        ],
        status: PlatformRuntimeStatus::Planned,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            message_edit,
            stream_update,
            cards,
            approval_buttons,
            threads,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            oauth,
            polling
        ),
        details: &[
            "Pub/Sub pull",
            "Chat REST outbound",
            "per-user OAuth file upload",
            "CARD_CLICKED routing",
        ],
    },
    PlatformMetadata {
        name: "irc",
        label: "IRC",
        source_channel: "irc",
        aliases: &[],
        required_env: &["IRC_SERVER", "IRC_NICKNAME", "IRC_CHANNEL"],
        optional_env: &[
            "IRC_PORT",
            "IRC_USE_TLS",
            "IRC_SERVER_PASSWORD",
            "IRC_NICKSERV_PASSWORD",
            "IRC_ALLOWED_USERS",
        ],
        env_aliases: &[
            "IRC_SERVER",
            "IRC_NICKNAME",
            "IRC_NICK",
            "IRC_CHANNEL",
            "IRC_CHANNELS",
            "FLYFLOR_IRC_PASSWORD",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(group_chat),
        details: &[
            "plain TCP IRC protocol",
            "PING/PONG",
            "channel and DM routing",
            "line chunking",
            "TLS/NickServ explicit unavailable",
        ],
    },
    PlatformMetadata {
        name: "ntfy",
        label: "ntfy",
        source_channel: "ntfy",
        aliases: &[],
        required_env: &["NTFY_TOPIC"],
        optional_env: &[
            "NTFY_SERVER_URL",
            "NTFY_TOKEN",
            "NTFY_PUBLISH_TOPIC",
            "NTFY_MARKDOWN",
            "NTFY_ALLOWED_USERS",
            "NTFY_HOME_CHANNEL",
        ],
        env_aliases: &[
            "NTFY_TOPIC",
            "NTFY_SERVER_URL",
            "NTFY_TOKEN",
            "NTFY_PUBLISH_TOPIC",
            "FLYFLOR_NTFY_TOKEN",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(long_poll),
        details: &[
            "HTTP streaming /json",
            "HTTP POST publish",
            "topic identity warning",
            "4096-char limit",
        ],
    },
    PlatformMetadata {
        name: "simplex",
        label: "SimpleX Chat",
        source_channel: "simplex",
        aliases: &["simplex-chat"],
        required_env: &["SIMPLEX_WS_URL"],
        optional_env: &[
            "SIMPLEX_ALLOWED_USERS",
            "SIMPLEX_ALLOW_ALL_USERS",
            "SIMPLEX_HOME_CHANNEL",
        ],
        env_aliases: &[
            "SIMPLEX_WS_URL",
            "SIMPLEX_ALLOWED_USERS",
            "SIMPLEX_CLI",
            "FLYFLOR_SIMPLEX_PASSWORD",
        ],
        status: PlatformRuntimeStatus::Planned,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            group_chat,
            file_download,
            file_upload,
            voice,
            websocket
        ),
        details: &[
            "simplex-chat daemon WS",
            "opaque contact ids",
            "own echo suppression",
            "media magic detection",
        ],
    },
    PlatformMetadata {
        name: "line",
        label: "LINE",
        source_channel: "line",
        aliases: &[],
        required_env: &["LINE_CHANNEL_ACCESS_TOKEN", "LINE_CHANNEL_SECRET"],
        optional_env: &[
            "LINE_ALLOWED_USERS",
            "LINE_HOME_CHANNEL",
            "LINE_SLOW_RESPONSE_THRESHOLD",
        ],
        env_aliases: &[
            "LINE_CHANNEL_ACCESS_TOKEN",
            "LINE_CHANNEL_SECRET",
            "FLYFLOR_LINE_CHANNEL_ACCESS_TOKEN",
            "FLYFLOR_LINE_CHANNEL_SECRET",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(group_chat, webhook_required),
        details: &[
            "text webhook payload",
            "reply token send",
            "push fallback",
            "media delivery unavailable",
        ],
    },
    PlatformMetadata {
        name: "mattermost",
        label: "Mattermost",
        source_channel: "mattermost",
        aliases: &[],
        required_env: &["MATTERMOST_URL", "MATTERMOST_TOKEN", "MATTERMOST_CHANNEL"],
        optional_env: &[
            "MATTERMOST_TEAM",
            "MATTERMOST_CHANNEL",
            "MATTERMOST_ALLOWED_USERS",
        ],
        env_aliases: &[
            "MATTERMOST_URL",
            "MATTERMOST_TOKEN",
            "MATTERMOST_TEAM",
            "FLYFLOR_MATTERMOST_TOKEN",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(group_chat, polling),
        details: &[
            "REST post polling",
            "REST create post",
            "channel routing",
            "media/edit/websocket explicit unavailable",
        ],
    },
    PlatformMetadata {
        name: "signal",
        label: "Signal",
        source_channel: "signal",
        aliases: &[],
        required_env: &["SIGNAL_PHONE_NUMBER"],
        optional_env: &[
            "SIGNAL_CLI_REST_API",
            "SIGNAL_ALLOWED_USERS",
            "SIGNAL_HOME_CHANNEL",
        ],
        env_aliases: &[
            "SIGNAL_CLI_REST_API",
            "SIGNAL_PHONE_NUMBER",
            "FLYFLOR_SIGNAL_PHONE_NUMBER",
        ],
        status: PlatformRuntimeStatus::Planned,
        capability: cap!(
            inbound_media,
            outbound_media,
            reactions,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            voice,
            service_install
        ),
        details: &[
            "Signal bridge",
            "group allowlists",
            "MEDIA tag delivery",
            "rate limit",
        ],
    },
    PlatformMetadata {
        name: "sms",
        label: "SMS",
        source_channel: "sms",
        aliases: &["sms-twilio", "twilio"],
        required_env: &[
            "TWILIO_ACCOUNT_SID",
            "TWILIO_AUTH_TOKEN",
            "TWILIO_FROM_NUMBER",
        ],
        optional_env: &["SMS_ALLOWED_USERS", "SMS_HOME_NUMBER"],
        env_aliases: &[
            "TWILIO_ACCOUNT_SID",
            "TWILIO_AUTH_TOKEN",
            "TWILIO_FROM_NUMBER",
            "FLYFLOR_TWILIO_ACCOUNT_SID",
            "FLYFLOR_TWILIO_AUTH_TOKEN",
            "FLYFLOR_TWILIO_FROM_NUMBER",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(webhook_required, direct_message),
        details: &[
            "Twilio inbound webhook payload",
            "Twilio Messages REST send",
            "media delivery unavailable",
        ],
    },
    PlatformMetadata {
        name: "bluebubbles",
        label: "BlueBubbles/iMessage",
        source_channel: "bluebubbles",
        aliases: &["bluebubbles-imessage", "imessage"],
        required_env: &["BLUEBUBBLES_SERVER_URL", "BLUEBUBBLES_PASSWORD"],
        optional_env: &[
            "BLUEBUBBLES_INBOUND_WEBHOOK",
            "BLUEBUBBLES_HOME_CHAT_GUID",
            "BLUEBUBBLES_SEND_METHOD",
            "BLUEBUBBLES_ALLOWED_USERS",
        ],
        env_aliases: &[
            "BLUEBUBBLES_SERVER_URL",
            "BLUEBUBBLES_PASSWORD",
            "FLYFLOR_BLUEBUBBLES_SERVER_URL",
            "FLYFLOR_BLUEBUBBLES_PASSWORD",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(group_chat, direct_message, webhook_required),
        details: &[
            "BlueBubbles webhook payload",
            "REST outbound text",
            "media/tapbacks/read receipts unavailable",
            "Private API degradation",
        ],
    },
    PlatformMetadata {
        name: "homeassistant",
        label: "Home Assistant",
        source_channel: "homeassistant",
        aliases: &["home-assistant", "hass"],
        required_env: &["HOME_ASSISTANT_URL", "HOME_ASSISTANT_TOKEN"],
        optional_env: &["HOME_ASSISTANT_WEBHOOK_SECRET"],
        env_aliases: &[
            "HOME_ASSISTANT_URL",
            "HOME_ASSISTANT_TOKEN",
            "HASS_URL",
            "HASS_TOKEN",
            "FLYFLOR_HOME_ASSISTANT_URL",
            "FLYFLOR_HOME_ASSISTANT_TOKEN",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(webhook_required, direct_message, polling),
        details: &[
            "local webhook ingest",
            "conversation/process reply",
            "notifications/entity routing unavailable",
        ],
    },
    PlatformMetadata {
        name: "open-webui",
        label: "Open WebUI",
        source_channel: "open-webui",
        aliases: &["openwebui"],
        required_env: &["OPEN_WEBUI_SECRET"],
        optional_env: &["OPEN_WEBUI_BIND", "OPEN_WEBUI_PUBLIC_URL"],
        env_aliases: &[
            "OPEN_WEBUI_SECRET",
            "OPEN_WEBUI_BIND",
            "OPEN_WEBUI_PUBLIC_URL",
            "FLYFLOR_OPEN_WEBUI_SECRET",
        ],
        status: PlatformRuntimeStatus::Native,
        capability: cap!(webhook_required, direct_message),
        details: &[
            "local webhook ingest",
            "callback reply",
            "file/media support unavailable",
        ],
    },
    PlatformMetadata {
        name: "yuanbao",
        label: "Yuanbao",
        source_channel: "yuanbao",
        aliases: &[],
        required_env: &["YUANBAO_APP_ID", "YUANBAO_APP_SECRET"],
        optional_env: &[
            "YUANBAO_WS_URL",
            "YUANBAO_API_DOMAIN",
            "YUANBAO_BOT_ID",
            "YUANBAO_ROUTE_ENV",
            "YUANBAO_HOME_CHANNEL",
        ],
        env_aliases: &[
            "YUANBAO_APP_ID",
            "YUANBAO_APP_SECRET",
            "YUANBAO_TOKEN",
            "YUANBAO_COOKIE",
            "FLYFLOR_YUANBAO_TOKEN",
        ],
        status: PlatformRuntimeStatus::Planned,
        capability: cap!(
            inbound_media,
            outbound_media,
            typing,
            reactions,
            group_chat,
            quote_reply,
            file_download,
            file_upload,
            voice,
            websocket
        ),
        details: &[
            "HMAC WebSocket",
            "C2C/group",
            "COS media",
            "heartbeat",
            "stickers/emoji",
        ],
    },
];

pub fn all_platforms() -> &'static [PlatformMetadata] {
    PLATFORMS
}

pub fn find_platform(input: &str) -> Option<&'static PlatformMetadata> {
    let normalized = normalize(input);
    PLATFORMS.iter().find(|platform| {
        platform.name == normalized
            || platform.source_channel == normalized
            || normalize(platform.label) == normalized
            || platform
                .aliases
                .iter()
                .any(|alias| normalize(alias) == normalized)
    })
}

pub fn canonical_platform_name(input: &str) -> Option<&'static str> {
    find_platform(input).map(|platform| platform.name)
}

fn normalize(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_source_and_human_aliases() {
        assert_eq!(canonical_platform_name("Weixin iLink"), Some("weixin"));
        assert_eq!(canonical_platform_name("feishu-lark"), Some("feishu"));
        assert_eq!(
            canonical_platform_name("BlueBubbles/iMessage"),
            Some("bluebubbles")
        );
        assert_eq!(canonical_platform_name("sms-twilio"), Some("sms"));
        assert_eq!(canonical_platform_name("Microsoft Teams"), Some("teams"));
        assert_eq!(
            canonical_platform_name("home-assistant"),
            Some("homeassistant")
        );
    }

    #[test]
    fn registry_matches_reference_messaging_surface() {
        let names = PLATFORMS
            .iter()
            .map(|platform| platform.name)
            .collect::<Vec<_>>();
        for expected in [
            "telegram",
            "discord",
            "slack",
            "matrix",
            "whatsapp",
            "feishu",
            "dingtalk",
            "wecom",
            "wecom-callback",
            "weixin",
            "qqbot",
            "email",
            "webhook",
            "teams",
            "msgraph-webhook",
            "google-chat",
            "irc",
            "ntfy",
            "simplex",
            "line",
            "mattermost",
            "signal",
            "sms",
            "bluebubbles",
            "homeassistant",
            "open-webui",
            "yuanbao",
        ] {
            assert!(names.contains(&expected), "{expected}");
        }
    }

    #[test]
    fn native_runtime_status_only_marks_implemented_adapters() {
        let native = PLATFORMS
            .iter()
            .filter(|platform| platform.native_runtime())
            .map(|platform| platform.name)
            .collect::<Vec<_>>();

        assert_eq!(
            native,
            vec![
                "telegram",
                "discord",
                "slack",
                "matrix",
                "whatsapp",
                "weixin",
                "email",
                "webhook",
                "irc",
                "ntfy",
                "line",
                "mattermost",
                "sms",
                "bluebubbles",
                "homeassistant",
                "open-webui"
            ]
        );
    }

    #[test]
    fn every_platform_has_env_and_capability_metadata() {
        assert_eq!(PLATFORMS.len(), 27);
        for platform in PLATFORMS {
            assert!(!platform.required_env.is_empty(), "{}", platform.name);
            assert!(!platform.env_aliases.is_empty(), "{}", platform.name);
            assert!(!platform.details.is_empty(), "{}", platform.name);
            assert!(platform.capability.inbound_text, "{}", platform.name);
            assert!(platform.capability.outbound_text, "{}", platform.name);
        }
    }
}
