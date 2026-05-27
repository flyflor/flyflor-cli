use std::{collections::HashMap, env, fs, path::PathBuf, sync::OnceLock};

use serde_json::Value;

static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();

pub fn text(key: CopyKey) -> String {
    text_key(key.as_str())
}

pub fn text_key(key: &str) -> String {
    CATALOG
        .get_or_init(load_catalog)
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

fn load_catalog() -> HashMap<String, String> {
    let lang = lang_code();
    external_catalog_paths(&lang)
        .into_iter()
        .find_map(|path| fs::read_to_string(path).ok())
        .and_then(|source| parse_catalog(&source))
        .unwrap_or_else(|| bundled_catalog(&lang))
}

fn external_catalog_paths(lang: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(path) = env::var("FLYFLOR_I18N_FILE") {
        paths.push(PathBuf::from(path));
    }
    if let Ok(dir) = env::var("FLYFLOR_I18N_DIR") {
        paths.push(PathBuf::from(dir).join(format!("{lang}.json")));
    }
    paths.push(PathBuf::from("i18n").join(format!("{lang}.json")));
    paths
}

fn bundled_catalog(lang: &str) -> HashMap<String, String> {
    let source = if lang.starts_with("en") {
        include_str!("../../i18n/en-US.json")
    } else {
        include_str!("../../i18n/zh-CN.json")
    };
    parse_catalog(source).unwrap_or_default()
}

fn parse_catalog(source: &str) -> Option<HashMap<String, String>> {
    let value: Value = serde_json::from_str(source).ok()?;
    let object = value.as_object()?;
    Some(
        object
            .iter()
            .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_string())))
            .collect(),
    )
}

fn lang_code() -> String {
    let raw = env::var("FLYFLOR_LANG").unwrap_or_else(|_| "zh-CN".to_string());
    let normalized = raw.replace('_', "-").to_ascii_lowercase();
    if normalized.starts_with("en") {
        "en-US".to_string()
    } else {
        "zh-CN".to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyKey {
    WaitingAsk,
    Running,
    Failed,
    Recorded,
    Models,
    Subagents,
    Tools,
    Processes,
    ChildId,
    Status,
    Limit,
    Task,
    Model,
    ModelAllocation,
    Batch,
    AllowedTools,
    Args,
    Partial,
    AskSuppressed,
    ExecutionEmpty,
    Result,
    Output,
    Error,
    AskReason,
    CrystalCandidate,
    Crystal,
    EnterSend,
    ShiftEnterNewline,
    ToggleMode,
    MoveCursor,
    CopySelection,
    Id,
    Proc,
    Duration,
    Selected,
    ApprovalNextTurn,
    HelpCommandText,
    ApprovalEnabled,
    ApprovalCancelled,
}

impl CopyKey {
    fn as_str(self) -> &'static str {
        match self {
            Self::WaitingAsk => "waitingAsk",
            Self::Running => "running",
            Self::Failed => "failed",
            Self::Recorded => "recorded",
            Self::Models => "models",
            Self::Subagents => "subagents",
            Self::Tools => "tools",
            Self::Processes => "processes",
            Self::ChildId => "childId",
            Self::Status => "status",
            Self::Limit => "limit",
            Self::Task => "task",
            Self::Model => "model",
            Self::ModelAllocation => "modelAllocation",
            Self::Batch => "batch",
            Self::AllowedTools => "allowedTools",
            Self::Args => "args",
            Self::Partial => "partial",
            Self::AskSuppressed => "askSuppressed",
            Self::ExecutionEmpty => "executionEmpty",
            Self::Result => "result",
            Self::Output => "output",
            Self::Error => "error",
            Self::AskReason => "askReason",
            Self::CrystalCandidate => "crystalCandidate",
            Self::Crystal => "crystal",
            Self::EnterSend => "enterSend",
            Self::ShiftEnterNewline => "shiftEnterNewline",
            Self::ToggleMode => "toggleMode",
            Self::MoveCursor => "moveCursor",
            Self::CopySelection => "copySelection",
            Self::Id => "id",
            Self::Proc => "proc",
            Self::Duration => "duration",
            Self::Selected => "selected",
            Self::ApprovalNextTurn => "approvalNextTurn",
            Self::HelpCommandText => "helpCommandText",
            Self::ApprovalEnabled => "approvalEnabled",
            Self::ApprovalCancelled => "approvalCancelled",
        }
    }
}
