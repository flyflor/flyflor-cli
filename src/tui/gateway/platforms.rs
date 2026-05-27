#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformMetadata {
    pub name: &'static str,
    pub label: &'static str,
    pub hermes_channel: &'static str,
    pub aliases: &'static [&'static str],
    pub env_aliases: &'static [&'static str],
    pub implemented: bool,
}

pub const PLATFORMS: &[PlatformMetadata] = &[
    PlatformMetadata {
        name: "telegram",
        label: "Telegram",
        hermes_channel: "telegram",
        aliases: &[],
        env_aliases: &[
            "TELEGRAM_BOT_TOKEN",
            "TELEGRAM_TOKEN",
            "TELEGRAM_CHAT_ID",
            "HERMES_TELEGRAM_BOT_TOKEN",
            "FLYFLOR_TELEGRAM_BOT_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "discord",
        label: "Discord",
        hermes_channel: "discord",
        aliases: &[],
        env_aliases: &[
            "DISCORD_TOKEN",
            "DISCORD_APPLICATION_ID",
            "DISCORD_PUBLIC_KEY",
            "HERMES_DISCORD_TOKEN",
            "FLYFLOR_DISCORD_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "whatsapp",
        label: "WhatsApp",
        hermes_channel: "whatsapp",
        aliases: &[],
        env_aliases: &[
            "WHATSAPP_TOKEN",
            "WHATSAPP_PHONE_NUMBER_ID",
            "WHATSAPP_VERIFY_TOKEN",
            "HERMES_WHATSAPP_TOKEN",
            "FLYFLOR_WHATSAPP_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "slack",
        label: "Slack",
        hermes_channel: "slack",
        aliases: &[],
        env_aliases: &[
            "SLACK_BOT_TOKEN",
            "SLACK_APP_TOKEN",
            "SLACK_SIGNING_SECRET",
            "HERMES_SLACK_BOT_TOKEN",
            "FLYFLOR_SLACK_BOT_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "signal",
        label: "Signal",
        hermes_channel: "signal",
        aliases: &[],
        env_aliases: &[
            "SIGNAL_CLI_REST_API",
            "SIGNAL_PHONE_NUMBER",
            "HERMES_SIGNAL_PHONE_NUMBER",
            "FLYFLOR_SIGNAL_PHONE_NUMBER",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "mattermost",
        label: "Mattermost",
        hermes_channel: "mattermost",
        aliases: &[],
        env_aliases: &[
            "MATTERMOST_URL",
            "MATTERMOST_TOKEN",
            "MATTERMOST_TEAM",
            "HERMES_MATTERMOST_TOKEN",
            "FLYFLOR_MATTERMOST_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "matrix",
        label: "Matrix",
        hermes_channel: "matrix",
        aliases: &[],
        env_aliases: &[
            "MATRIX_HOMESERVER",
            "MATRIX_ACCESS_TOKEN",
            "MATRIX_USER_ID",
            "HERMES_MATRIX_ACCESS_TOKEN",
            "FLYFLOR_MATRIX_ACCESS_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "home-assistant",
        label: "Home Assistant",
        hermes_channel: "home-assistant",
        aliases: &["homeassistant", "hass"],
        env_aliases: &[
            "HOME_ASSISTANT_URL",
            "HOME_ASSISTANT_TOKEN",
            "HASS_URL",
            "HASS_TOKEN",
            "HERMES_HOME_ASSISTANT_TOKEN",
            "FLYFLOR_HOME_ASSISTANT_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "email",
        label: "Email",
        hermes_channel: "email",
        aliases: &["smtp", "imap"],
        env_aliases: &[
            "EMAIL_SMTP_URL",
            "EMAIL_IMAP_URL",
            "EMAIL_USERNAME",
            "EMAIL_PASSWORD",
            "HERMES_EMAIL_PASSWORD",
            "FLYFLOR_EMAIL_PASSWORD",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "sms-twilio",
        label: "SMS/Twilio",
        hermes_channel: "sms-twilio",
        aliases: &["sms", "twilio"],
        env_aliases: &[
            "TWILIO_ACCOUNT_SID",
            "TWILIO_AUTH_TOKEN",
            "TWILIO_FROM_NUMBER",
            "HERMES_TWILIO_AUTH_TOKEN",
            "FLYFLOR_TWILIO_AUTH_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "dingtalk",
        label: "DingTalk",
        hermes_channel: "dingtalk",
        aliases: &["ding-talk"],
        env_aliases: &[
            "DINGTALK_APP_KEY",
            "DINGTALK_APP_SECRET",
            "DINGTALK_ROBOT_CODE",
            "HERMES_DINGTALK_APP_SECRET",
            "FLYFLOR_DINGTALK_APP_SECRET",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "feishu-lark",
        label: "Feishu/Lark",
        hermes_channel: "feishu-lark",
        aliases: &["feishu", "lark"],
        env_aliases: &[
            "FEISHU_APP_ID",
            "FEISHU_APP_SECRET",
            "LARK_APP_ID",
            "LARK_APP_SECRET",
            "HERMES_FEISHU_APP_SECRET",
            "FLYFLOR_FEISHU_APP_SECRET",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "wecom",
        label: "WeCom",
        hermes_channel: "wecom",
        aliases: &["wechat-work"],
        env_aliases: &[
            "WECOM_CORP_ID",
            "WECOM_AGENT_ID",
            "WECOM_SECRET",
            "HERMES_WECOM_SECRET",
            "FLYFLOR_WECOM_SECRET",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "wecom-callback",
        label: "WeCom Callback",
        hermes_channel: "wecom-callback",
        aliases: &[],
        env_aliases: &[
            "WECOM_CALLBACK_TOKEN",
            "WECOM_CALLBACK_AES_KEY",
            "WECOM_CORP_ID",
            "HERMES_WECOM_CALLBACK_TOKEN",
            "FLYFLOR_WECOM_CALLBACK_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "weixin",
        label: "Weixin iLink",
        hermes_channel: "weixin-ilink",
        aliases: &["weixin-ilink", "wechat", "ilink"],
        env_aliases: &[
            "WEIXIN_ACCOUNT_ID",
            "WEIXIN_TOKEN",
            "WEIXIN_BASE_URL",
            "WEIXIN_DM_POLICY",
            "WEIXIN_GROUP_POLICY",
            "FLYFLOR_WEIXIN_ACCOUNT_ID",
            "FLYFLOR_WEIXIN_TOKEN",
            "FLYFLOR_WEIXIN_BASE_URL",
            "HERMES_WEIXIN_TOKEN",
        ],
        implemented: true,
    },
    PlatformMetadata {
        name: "bluebubbles-imessage",
        label: "BlueBubbles/iMessage",
        hermes_channel: "bluebubbles-imessage",
        aliases: &["bluebubbles", "imessage"],
        env_aliases: &[
            "BLUEBUBBLES_SERVER_URL",
            "BLUEBUBBLES_PASSWORD",
            "HERMES_BLUEBUBBLES_PASSWORD",
            "FLYFLOR_BLUEBUBBLES_PASSWORD",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "qqbot",
        label: "QQBot",
        hermes_channel: "qqbot",
        aliases: &["qq"],
        env_aliases: &[
            "QQBOT_APP_ID",
            "QQBOT_SECRET",
            "QQBOT_TOKEN",
            "HERMES_QQBOT_TOKEN",
            "FLYFLOR_QQBOT_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "yuanbao",
        label: "Yuanbao",
        hermes_channel: "yuanbao",
        aliases: &[],
        env_aliases: &[
            "YUANBAO_TOKEN",
            "YUANBAO_COOKIE",
            "HERMES_YUANBAO_TOKEN",
            "FLYFLOR_YUANBAO_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "api",
        label: "API Server",
        hermes_channel: "api-server",
        aliases: &["api-server"],
        env_aliases: &[
            "GATEWAY_API_TOKEN",
            "GATEWAY_API_BIND",
            "HERMES_API_TOKEN",
            "FLYFLOR_GATEWAY_API_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "webhook",
        label: "Webhook",
        hermes_channel: "webhook",
        aliases: &[],
        env_aliases: &[
            "WEBHOOK_SECRET",
            "WEBHOOK_BIND",
            "WEBHOOK_PUBLIC_URL",
            "HERMES_WEBHOOK_SECRET",
            "FLYFLOR_WEBHOOK_SECRET",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "msgraph-webhook",
        label: "MSGraph Webhook",
        hermes_channel: "msgraph-webhook",
        aliases: &["msgraph", "microsoft-graph"],
        env_aliases: &[
            "MSGRAPH_TENANT_ID",
            "MSGRAPH_CLIENT_ID",
            "MSGRAPH_CLIENT_SECRET",
            "MSGRAPH_WEBHOOK_SECRET",
            "HERMES_MSGRAPH_CLIENT_SECRET",
            "FLYFLOR_MSGRAPH_CLIENT_SECRET",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "google-chat",
        label: "Google Chat",
        hermes_channel: "google-chat",
        aliases: &["gchat"],
        env_aliases: &[
            "GOOGLE_CHAT_PROJECT_ID",
            "GOOGLE_CHAT_SERVICE_ACCOUNT",
            "GOOGLE_CHAT_WEBHOOK_URL",
            "HERMES_GOOGLE_CHAT_SERVICE_ACCOUNT",
            "FLYFLOR_GOOGLE_CHAT_SERVICE_ACCOUNT",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "irc",
        label: "IRC",
        hermes_channel: "irc",
        aliases: &[],
        env_aliases: &[
            "IRC_SERVER",
            "IRC_NICK",
            "IRC_PASSWORD",
            "IRC_CHANNELS",
            "HERMES_IRC_PASSWORD",
            "FLYFLOR_IRC_PASSWORD",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "line",
        label: "LINE",
        hermes_channel: "line",
        aliases: &[],
        env_aliases: &[
            "LINE_CHANNEL_ACCESS_TOKEN",
            "LINE_CHANNEL_SECRET",
            "HERMES_LINE_CHANNEL_ACCESS_TOKEN",
            "FLYFLOR_LINE_CHANNEL_ACCESS_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "ntfy",
        label: "ntfy",
        hermes_channel: "ntfy",
        aliases: &[],
        env_aliases: &[
            "NTFY_URL",
            "NTFY_TOPIC",
            "NTFY_TOKEN",
            "HERMES_NTFY_TOKEN",
            "FLYFLOR_NTFY_TOKEN",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "simplex",
        label: "SimpleX",
        hermes_channel: "simplex",
        aliases: &["simplex-chat"],
        env_aliases: &[
            "SIMPLEX_CLI",
            "SIMPLEX_PROFILE",
            "SIMPLEX_PASSWORD",
            "HERMES_SIMPLEX_PASSWORD",
            "FLYFLOR_SIMPLEX_PASSWORD",
        ],
        implemented: false,
    },
    PlatformMetadata {
        name: "microsoft-teams",
        label: "Microsoft Teams",
        hermes_channel: "microsoft-teams",
        aliases: &["teams", "ms-teams"],
        env_aliases: &[
            "TEAMS_TENANT_ID",
            "TEAMS_CLIENT_ID",
            "TEAMS_CLIENT_SECRET",
            "TEAMS_WEBHOOK_URL",
            "HERMES_TEAMS_CLIENT_SECRET",
            "FLYFLOR_TEAMS_CLIENT_SECRET",
        ],
        implemented: false,
    },
];

pub fn all_platforms() -> &'static [PlatformMetadata] {
    PLATFORMS
}

pub fn find_platform(input: &str) -> Option<&'static PlatformMetadata> {
    let normalized = normalize(input);
    PLATFORMS.iter().find(|platform| {
        platform.name == normalized
            || platform.hermes_channel == normalized
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
    fn resolves_hermes_and_human_aliases() {
        assert_eq!(canonical_platform_name("Weixin iLink"), Some("weixin"));
        assert_eq!(canonical_platform_name("api-server"), Some("api"));
        assert_eq!(
            canonical_platform_name("BlueBubbles/iMessage"),
            Some("bluebubbles-imessage")
        );
    }

    #[test]
    fn every_platform_has_hermes_env_alias_metadata() {
        assert_eq!(PLATFORMS.len(), 27);
        for platform in PLATFORMS {
            assert!(!platform.env_aliases.is_empty(), "{}", platform.name);
            assert!(
                platform
                    .env_aliases
                    .iter()
                    .any(|alias| alias.starts_with("HERMES_")),
                "{}",
                platform.name
            );
        }
    }
}
