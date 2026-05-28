use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::tui::gateway::platforms::{PlatformMetadata, all_platforms, canonical_platform_name};

const CLI_HOME_ENV: &str = "FLYFLOR_CLI_HOME";
const CONFIG_PATH_ENV: &str = "FLYFLOR_GATEWAY_CONFIG";
const CONFIG_FILE: &str = "gateway.jsonc";

#[derive(Debug)]
pub enum GatewayConfigError {
    Io(io::Error),
    Parse(String),
    Validation(Vec<String>),
    UnknownPlatform(String),
}

impl std::fmt::Display for GatewayConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Parse(error) => write!(formatter, "gateway config parse error: {error}"),
            Self::Validation(errors) => {
                write!(formatter, "gateway config validation failed")?;
                for error in errors {
                    write!(formatter, "\n- {error}")?;
                }
                Ok(())
            }
            Self::UnknownPlatform(platform) => {
                write!(formatter, "unknown gateway platform: {platform}")
            }
        }
    }
}

impl std::error::Error for GatewayConfigError {}

impl From<io::Error> for GatewayConfigError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigWriteReport {
    pub path: PathBuf,
    pub created: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationReport {
    pub path: PathBuf,
    pub enabled_channels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelToggleReport {
    pub path: PathBuf,
    pub platform: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelListItem {
    pub name: &'static str,
    pub label: &'static str,
    pub source_channel: &'static str,
    pub enabled: bool,
    pub native_runtime: bool,
    pub status: &'static str,
    pub features: Vec<&'static str>,
    pub details: &'static [&'static str],
    pub required_env: &'static [&'static str],
    pub optional_env: &'static [&'static str],
    pub env_aliases: &'static [&'static str],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelDoctorItem {
    pub name: &'static str,
    pub enabled: bool,
    pub native_runtime: bool,
    pub status: &'static str,
    pub availability: ChannelAvailability,
    pub features: Vec<&'static str>,
    pub details: &'static [&'static str],
    pub present_env_aliases: Vec<&'static str>,
    pub missing_env_aliases: Vec<&'static str>,
    pub present_required_env: Vec<&'static str>,
    pub missing_required_env: Vec<&'static str>,
    pub optional_env: &'static [&'static str],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelAvailability {
    Available,
    Unavailable,
}

impl ChannelAvailability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelDoctorReport {
    pub path: PathBuf,
    pub validation: ValidationReport,
    pub channels: Vec<ChannelDoctorItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GatewayConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub core: CoreConfig,
    #[serde(default)]
    pub gateway: GatewaySection,
    #[serde(default)]
    pub streaming: StreamingSection,
    #[serde(default)]
    pub display: DisplaySection,
    #[serde(default = "default_platforms")]
    pub platforms: BTreeMap<String, PlatformConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CoreConfig {
    #[serde(default = "default_ws_url")]
    pub ws_url: String,
    #[serde(default = "default_client_id")]
    pub client_id: String,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            ws_url: default_ws_url(),
            client_id: default_client_id(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GatewaySection {
    #[serde(default)]
    pub enabled_channels: Vec<String>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for GatewaySection {
    fn default() -> Self {
        Self {
            enabled_channels: Vec::new(),
            log_level: default_log_level(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamingSection {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub typing_indicator: bool,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
}

impl Default for StreamingSection {
    fn default() -> Self {
        Self {
            enabled: true,
            typing_indicator: true,
            poll_interval_ms: default_poll_interval_ms(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DisplaySection {
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default = "default_true")]
    pub show_platform_labels: bool,
}

impl Default for DisplaySection {
    fn default() -> Self {
        Self {
            locale: default_locale(),
            show_platform_labels: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub settings: Map<String, Value>,
}

impl Default for PlatformConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            env: BTreeMap::new(),
            settings: Map::new(),
        }
    }
}

pub fn config_path_from_env() -> PathBuf {
    env::var(CONFIG_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| cli_home_from_env().join(CONFIG_FILE))
}

pub fn init_default() -> Result<ConfigWriteReport, GatewayConfigError> {
    init_at(&config_path_from_env())
}

pub fn show_default() -> Result<String, GatewayConfigError> {
    Ok(fs::read_to_string(config_path_from_env())?)
}

pub fn validate_default() -> Result<ValidationReport, GatewayConfigError> {
    validate_file(&config_path_from_env())
}

pub fn list_channels_default() -> Result<Vec<ChannelListItem>, GatewayConfigError> {
    let config = read_config_if_exists(&config_path_from_env())?.unwrap_or_default();
    Ok(channel_list(&config))
}

pub fn doctor_default() -> Result<ChannelDoctorReport, GatewayConfigError> {
    doctor_at(&config_path_from_env())
}

pub fn doctor_channel_default(platform: &str) -> Result<ChannelDoctorItem, GatewayConfigError> {
    let config = read_config_if_exists(&config_path_from_env())?.unwrap_or_default();
    doctor_channel_in_config(&config, platform)
}

pub fn set_channel_default(
    platform: &str,
    enabled: bool,
) -> Result<ChannelToggleReport, GatewayConfigError> {
    set_channel_at(&config_path_from_env(), platform, enabled)
}

pub fn enabled_channel_names_from_default_config() -> Vec<String> {
    read_config_if_exists(&config_path_from_env())
        .ok()
        .flatten()
        .map(|config| enabled_channel_names(&config))
        .unwrap_or_default()
}

pub fn init_at(path: &Path) -> Result<ConfigWriteReport, GatewayConfigError> {
    if path.exists() {
        return Ok(ConfigWriteReport {
            path: path.to_path_buf(),
            created: false,
        });
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, to_jsonc(&GatewayConfig::default()))?;
    Ok(ConfigWriteReport {
        path: path.to_path_buf(),
        created: true,
    })
}

pub fn validate_file(path: &Path) -> Result<ValidationReport, GatewayConfigError> {
    let config = read_config(path)?;
    let errors = validate_config(&config, Some(&fs::read_to_string(path)?));
    if errors.is_empty() {
        Ok(ValidationReport {
            path: path.to_path_buf(),
            enabled_channels: enabled_channel_names(&config),
        })
    } else {
        Err(GatewayConfigError::Validation(errors))
    }
}

pub fn doctor_at(path: &Path) -> Result<ChannelDoctorReport, GatewayConfigError> {
    let validation = validate_file(path)?;
    let config = read_config(path)?;
    let channels = channel_list(&config)
        .into_iter()
        .filter(|item| item.enabled)
        .map(doctor_item_from_list_item)
        .collect();
    Ok(ChannelDoctorReport {
        path: path.to_path_buf(),
        validation,
        channels,
    })
}

fn doctor_channel_in_config(
    config: &GatewayConfig,
    platform: &str,
) -> Result<ChannelDoctorItem, GatewayConfigError> {
    let Some(canonical) = canonical_platform_name(platform) else {
        return Err(GatewayConfigError::UnknownPlatform(platform.to_string()));
    };
    channel_list(config)
        .into_iter()
        .find(|item| item.name == canonical)
        .map(doctor_item_from_list_item)
        .ok_or_else(|| GatewayConfigError::UnknownPlatform(platform.to_string()))
}

fn doctor_item_from_list_item(item: ChannelListItem) -> ChannelDoctorItem {
    doctor_item_from_list_item_with_env(item, env_present)
}

fn doctor_item_from_list_item_with_env(
    item: ChannelListItem,
    mut has_env: impl FnMut(&str) -> bool,
) -> ChannelDoctorItem {
    let (present_env_aliases, missing_env_aliases) = item
        .env_aliases
        .iter()
        .copied()
        .partition(|alias| has_env(alias));
    let present_required_env = item
        .required_env
        .iter()
        .copied()
        .filter(|alias| has_env(alias))
        .collect::<Vec<_>>();
    let missing_required_env = item
        .required_env
        .iter()
        .copied()
        .filter(|alias| !has_env(alias))
        .collect::<Vec<_>>();
    let availability = if item.native_runtime && missing_required_env.is_empty() {
        ChannelAvailability::Available
    } else {
        ChannelAvailability::Unavailable
    };
    ChannelDoctorItem {
        name: item.name,
        enabled: item.enabled,
        native_runtime: item.native_runtime,
        status: item.status,
        availability,
        features: item.features,
        details: item.details,
        present_env_aliases,
        missing_env_aliases,
        present_required_env,
        missing_required_env,
        optional_env: item.optional_env,
    }
}

fn env_present(alias: &str) -> bool {
    env::var(alias).is_ok_and(|value| !value.trim().is_empty())
}

pub fn set_channel_at(
    path: &Path,
    platform: &str,
    enabled: bool,
) -> Result<ChannelToggleReport, GatewayConfigError> {
    let Some(canonical) = canonical_platform_name(platform) else {
        return Err(GatewayConfigError::UnknownPlatform(platform.to_string()));
    };
    let mut config = read_config_if_exists(path)?.unwrap_or_default();
    config
        .platforms
        .entry(canonical.to_string())
        .or_default()
        .enabled = enabled;
    if enabled {
        if !config
            .gateway
            .enabled_channels
            .iter()
            .any(|name| name == canonical)
        {
            config.gateway.enabled_channels.push(canonical.to_string());
        }
    } else {
        config
            .gateway
            .enabled_channels
            .retain(|name| canonical_platform_name(name).is_some_and(|name| name != canonical));
    }
    config.gateway.enabled_channels.sort();
    let errors = validate_config(&config, None);
    if !errors.is_empty() {
        return Err(GatewayConfigError::Validation(errors));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, to_jsonc(&config))?;
    Ok(ChannelToggleReport {
        path: path.to_path_buf(),
        platform: canonical.to_string(),
        enabled,
    })
}

fn cli_home_from_env() -> PathBuf {
    env::var(CLI_HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            env::var("HOME")
                .map(|home| PathBuf::from(home).join(".flyflor-cli"))
                .unwrap_or_else(|_| PathBuf::from(".flyflor-cli"))
        })
}

fn read_config_if_exists(path: &Path) -> Result<Option<GatewayConfig>, GatewayConfigError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(parse_jsonc(&text)?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(GatewayConfigError::Io(error)),
    }
}

fn read_config(path: &Path) -> Result<GatewayConfig, GatewayConfigError> {
    parse_jsonc(&fs::read_to_string(path)?)
}

fn parse_jsonc(text: &str) -> Result<GatewayConfig, GatewayConfigError> {
    let json = strip_trailing_commas(&strip_jsonc_comments(text));
    serde_json::from_str::<GatewayConfig>(&json)
        .map_err(|error| GatewayConfigError::Parse(error.to_string()))
}

fn validate_config(config: &GatewayConfig, raw: Option<&str>) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(raw) = raw
        && let Ok(value) =
            serde_json::from_str::<Value>(&strip_trailing_commas(&strip_jsonc_comments(raw)))
        && (value.get("session").is_some() || value.get("sessions").is_some())
    {
        errors.push("session config fields are not allowed in gateway JSONC".to_string());
    }
    if config.schema_version == 0 {
        errors.push("schemaVersion must be greater than zero".to_string());
    }
    for channel in &config.gateway.enabled_channels {
        if canonical_platform_name(channel).is_none() {
            errors.push(format!(
                "gateway.enabledChannels includes unknown platform {channel}"
            ));
        }
    }
    for name in config.platforms.keys() {
        if canonical_platform_name(name).is_none() {
            errors.push(format!("platforms includes unknown platform {name}"));
        }
    }
    for name in enabled_channel_names(config) {
        if !config.platforms.contains_key(&name) {
            errors.push(format!("enabled platform {name} is missing from platforms"));
        }
    }
    errors
}

fn channel_list(config: &GatewayConfig) -> Vec<ChannelListItem> {
    all_platforms()
        .iter()
        .map(|platform| ChannelListItem {
            name: platform.name,
            label: platform.label,
            source_channel: platform.source_channel,
            enabled: is_platform_enabled(config, platform),
            native_runtime: platform.native_runtime(),
            status: platform.status.as_str(),
            features: platform.capability.feature_names(),
            details: platform.details,
            required_env: platform.required_env,
            optional_env: platform.optional_env,
            env_aliases: platform.env_aliases,
        })
        .collect()
}

fn enabled_channel_names(config: &GatewayConfig) -> Vec<String> {
    let mut names = all_platforms()
        .iter()
        .filter(|platform| is_platform_enabled(config, platform))
        .map(|platform| platform.name.to_string())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn is_platform_enabled(config: &GatewayConfig, platform: &PlatformMetadata) -> bool {
    config
        .platforms
        .get(platform.name)
        .is_some_and(|platform| platform.enabled)
        || config.gateway.enabled_channels.iter().any(|name| {
            canonical_platform_name(name).is_some_and(|canonical| canonical == platform.name)
        })
}

fn to_jsonc(config: &GatewayConfig) -> String {
    let value = serde_json::to_string_pretty(config).unwrap_or_else(|_| "{}".to_string());
    format!(
        "\
// Flyflor CLI gateway config. JSONC is the only supported gateway config format.
// Runtime state stays under the CLI home; this file does not define session fields.
{value}
"
    )
}

fn strip_jsonc_comments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }
        if ch == '/'
            && let Some(next) = chars.peek().copied()
        {
            if next == '/' {
                chars.next();
                for comment_ch in chars.by_ref() {
                    if comment_ch == '\n' {
                        output.push('\n');
                        break;
                    }
                }
                continue;
            }
            if next == '*' {
                chars.next();
                let mut previous = '\0';
                for comment_ch in chars.by_ref() {
                    if comment_ch == '\n' {
                        output.push('\n');
                    }
                    if previous == '*' && comment_ch == '/' {
                        break;
                    }
                    previous = comment_ch;
                }
                continue;
            }
        }
        output.push(ch);
    }
    output
}

fn strip_trailing_commas(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let chars = text.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if ch == '"' {
            in_string = true;
            output.push(ch);
            index += 1;
            continue;
        }
        if ch == ',' {
            let mut lookahead = index + 1;
            while lookahead < chars.len() && chars[lookahead].is_whitespace() {
                lookahead += 1;
            }
            if lookahead < chars.len() && matches!(chars[lookahead], '}' | ']') {
                index += 1;
                continue;
            }
        }
        output.push(ch);
        index += 1;
    }
    output
}

fn default_platforms() -> BTreeMap<String, PlatformConfig> {
    all_platforms()
        .iter()
        .map(|platform| (platform.name.to_string(), PlatformConfig::default()))
        .collect()
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            core: CoreConfig::default(),
            gateway: GatewaySection::default(),
            streaming: StreamingSection::default(),
            display: DisplaySection::default(),
            platforms: default_platforms(),
        }
    }
}

fn default_schema_version() -> u32 {
    1
}

fn default_ws_url() -> String {
    "ws://127.0.0.1:8787/ws".to_string()
}

fn default_client_id() -> String {
    "flyflor-cli-gateway".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_locale() -> String {
    "auto".to_string()
}

fn default_true() -> bool {
    true
}

fn default_poll_interval_ms() -> u64 {
    1_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jsonc_comments_and_trailing_commas() {
        let config = parse_jsonc(
            r#"
            {
              // gateway channels may use source aliases
              "schemaVersion": 1,
              "gateway": { "enabledChannels": ["weixin-ilink",], },
              "platforms": {
                "weixin": { "enabled": true, },
              },
            }
            "#,
        )
        .unwrap();

        assert_eq!(config.schema_version, 1);
        assert!(config.platforms["weixin"].enabled);
        assert_eq!(enabled_channel_names(&config), vec!["weixin"]);
    }

    #[test]
    fn init_writes_default_jsonc_schema() {
        let path = temp_config_path("init");
        let report = init_at(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let config = parse_jsonc(&text).unwrap();

        assert!(report.created);
        assert!(text.contains("JSONC is the only supported"));
        assert!(config.platforms.contains_key("telegram"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn validate_rejects_session_fields_and_unknown_platforms() {
        let path = temp_config_path("validate");
        fs::write(
            &path,
            r#"{
              "schemaVersion": 1,
              "session": { "id": "bad" },
              "gateway": { "enabledChannels": ["not-real"] },
              "platforms": { "telegram": { "enabled": true } }
            }"#,
        )
        .unwrap();

        let error = validate_file(&path).unwrap_err().to_string();

        assert!(error.contains("session config fields"));
        assert!(error.contains("not-real"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn enable_disable_rewrites_minimal_jsonc() {
        let path = temp_config_path("toggle");
        set_channel_at(&path, "weixin-ilink", true).unwrap();
        let enabled = read_config(&path).unwrap();

        assert!(enabled.platforms["weixin"].enabled);
        assert_eq!(enabled_channel_names(&enabled), vec!["weixin"]);

        set_channel_at(&path, "weixin", false).unwrap();
        let disabled = read_config(&path).unwrap();

        assert!(!disabled.platforms["weixin"].enabled);
        assert!(enabled_channel_names(&disabled).is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn single_channel_doctor_reports_unavailable_without_required_env() {
        let config = GatewayConfig::default();
        let weixin = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "weixin")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(weixin, |_| false);

        assert_eq!(item.name, "weixin");
        assert_eq!(item.availability, ChannelAvailability::Unavailable);
        assert_eq!(item.missing_required_env, vec!["WEIXIN_ACCOUNT_ID"]);
        assert!(item.native_runtime);
    }

    #[test]
    fn all_channels_are_unavailable_when_required_env_is_missing() {
        let config = GatewayConfig::default();
        let items = channel_list(&config)
            .into_iter()
            .map(|item| doctor_item_from_list_item_with_env(item, |_| false))
            .collect::<Vec<_>>();

        assert_eq!(items.len(), 27);
        assert!(
            items
                .iter()
                .all(|item| item.availability == ChannelAvailability::Unavailable)
        );
        assert!(
            items
                .iter()
                .all(|item| !item.missing_required_env.is_empty())
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "weixin" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "telegram" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "discord" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "webhook" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "ntfy" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "matrix" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "whatsapp" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "irc" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "mattermost" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "email" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "homeassistant" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "open-webui" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "sms" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "line" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "bluebubbles" && item.native_runtime)
        );
        assert!(
            items
                .iter()
                .any(|item| item.name == "slack" && item.native_runtime)
        );
    }

    #[test]
    fn planned_channel_stays_unavailable_even_with_env_present() {
        let config = GatewayConfig::default();
        let teams = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "teams")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(teams, |_| true);

        assert_eq!(item.availability, ChannelAvailability::Unavailable);
        assert!(!item.native_runtime);
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn telegram_native_channel_is_available_when_token_is_present() {
        let config = GatewayConfig::default();
        let telegram = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "telegram")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(telegram, |env| env == "TELEGRAM_BOT_TOKEN");

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(item.present_required_env, vec!["TELEGRAM_BOT_TOKEN"]);
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn discord_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let discord = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "discord")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(discord, |env| {
            matches!(env, "DISCORD_BOT_TOKEN" | "DISCORD_HOME_CHANNEL")
        });

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(
            item.present_required_env,
            vec!["DISCORD_BOT_TOKEN", "DISCORD_HOME_CHANNEL"]
        );
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn slack_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let slack = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "slack")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(slack, |env| {
            matches!(env, "SLACK_BOT_TOKEN" | "SLACK_HOME_CHANNEL")
        });

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(
            item.present_required_env,
            vec!["SLACK_BOT_TOKEN", "SLACK_HOME_CHANNEL"]
        );
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn webhook_native_channel_is_available_when_secret_is_present() {
        let config = GatewayConfig::default();
        let webhook = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "webhook")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(webhook, |env| env == "WEBHOOK_SECRET");

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(item.present_required_env, vec!["WEBHOOK_SECRET"]);
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn ntfy_native_channel_is_available_when_topic_is_present() {
        let config = GatewayConfig::default();
        let ntfy = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "ntfy")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(ntfy, |env| env == "NTFY_TOPIC");

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(item.present_required_env, vec!["NTFY_TOPIC"]);
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn matrix_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let matrix = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "matrix")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(matrix, |env| {
            matches!(
                env,
                "MATRIX_HOMESERVER" | "MATRIX_ACCESS_TOKEN" | "MATRIX_USER_ID"
            )
        });

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(
            item.present_required_env,
            vec!["MATRIX_HOMESERVER", "MATRIX_ACCESS_TOKEN", "MATRIX_USER_ID"]
        );
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn whatsapp_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let whatsapp = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "whatsapp")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(whatsapp, |env| {
            matches!(env, "WHATSAPP_ACCESS_TOKEN" | "WHATSAPP_PHONE_NUMBER_ID")
        });

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(
            item.present_required_env,
            vec!["WHATSAPP_ACCESS_TOKEN", "WHATSAPP_PHONE_NUMBER_ID"]
        );
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn irc_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let irc = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "irc")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(irc, |env| {
            matches!(env, "IRC_SERVER" | "IRC_NICKNAME" | "IRC_CHANNEL")
        });

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(
            item.present_required_env,
            vec!["IRC_SERVER", "IRC_NICKNAME", "IRC_CHANNEL"]
        );
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn mattermost_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let mattermost = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "mattermost")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(mattermost, |env| {
            matches!(
                env,
                "MATTERMOST_URL" | "MATTERMOST_TOKEN" | "MATTERMOST_CHANNEL"
            )
        });

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(
            item.present_required_env,
            vec!["MATTERMOST_URL", "MATTERMOST_TOKEN", "MATTERMOST_CHANNEL"]
        );
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn email_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let email = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "email")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(email, |env| {
            matches!(env, "EMAIL_ADDRESS" | "EMAIL_PASSWORD" | "EMAIL_SMTP_HOST")
        });

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(
            item.present_required_env,
            vec!["EMAIL_ADDRESS", "EMAIL_PASSWORD", "EMAIL_SMTP_HOST"]
        );
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn homeassistant_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let homeassistant = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "homeassistant")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(homeassistant, |env| {
            matches!(env, "HOME_ASSISTANT_URL" | "HOME_ASSISTANT_TOKEN")
        });

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(
            item.present_required_env,
            vec!["HOME_ASSISTANT_URL", "HOME_ASSISTANT_TOKEN"]
        );
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn openwebui_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let openwebui = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "open-webui")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(openwebui, |env| env == "OPEN_WEBUI_SECRET");

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(item.present_required_env, vec!["OPEN_WEBUI_SECRET"]);
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn sms_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let sms = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "sms")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(sms, |env| {
            matches!(
                env,
                "TWILIO_ACCOUNT_SID" | "TWILIO_AUTH_TOKEN" | "TWILIO_FROM_NUMBER"
            )
        });

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(
            item.present_required_env,
            vec![
                "TWILIO_ACCOUNT_SID",
                "TWILIO_AUTH_TOKEN",
                "TWILIO_FROM_NUMBER"
            ]
        );
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn line_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let line = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "line")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(line, |env| {
            matches!(env, "LINE_CHANNEL_ACCESS_TOKEN" | "LINE_CHANNEL_SECRET")
        });

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(
            item.present_required_env,
            vec!["LINE_CHANNEL_ACCESS_TOKEN", "LINE_CHANNEL_SECRET"]
        );
        assert!(item.missing_required_env.is_empty());
    }

    #[test]
    fn bluebubbles_native_channel_is_available_when_required_env_is_present() {
        let config = GatewayConfig::default();
        let bluebubbles = channel_list(&config)
            .into_iter()
            .find(|item| item.name == "bluebubbles")
            .unwrap();
        let item = doctor_item_from_list_item_with_env(bluebubbles, |env| {
            matches!(env, "BLUEBUBBLES_SERVER_URL" | "BLUEBUBBLES_PASSWORD")
        });

        assert_eq!(item.availability, ChannelAvailability::Available);
        assert!(item.native_runtime);
        assert_eq!(
            item.present_required_env,
            vec!["BLUEBUBBLES_SERVER_URL", "BLUEBUBBLES_PASSWORD"]
        );
        assert!(item.missing_required_env.is_empty());
    }

    fn temp_config_path(label: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "flyflor-gateway-config-{label}-{}-{}.jsonc",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }
}
