use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentStatus {
    Pending,
    Running,
    NeedsUser,
    Completed,
    Failed,
    Unknown,
}

impl SubagentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::NeedsUser => "needs_user",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_value(value: &Value) -> Self {
        if value
            .get("needsUser")
            .or_else(|| value.get("needs_user"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Self::NeedsUser;
        }

        match value
            .get("status")
            .or_else(|| value.get("state"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "pending" | "queued" => Self::Pending,
            "running" | "started" | "in_progress" | "in-progress" => Self::Running,
            "needs_user" | "needs-user" | "paused" | "waiting_for_user" => Self::NeedsUser,
            "completed" | "complete" | "succeeded" | "success" | "done" => Self::Completed,
            "failed" | "error" | "cancelled" | "canceled" => Self::Failed,
            _ => Self::Unknown,
        }
    }

    pub fn from_event_type(event_type: &str) -> Self {
        if event_type.ends_with(".start") || event_type.ends_with(".started") {
            Self::Running
        } else if event_type.ends_with(".end")
            || event_type.ends_with(".ended")
            || event_type.ends_with(".completed")
        {
            Self::Completed
        } else if event_type.ends_with(".failed") || event_type.ends_with(".error") {
            Self::Failed
        } else {
            Self::Unknown
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentToolCall {
    pub id: String,
    pub name: String,
    pub status: SubagentStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentChild {
    pub id: String,
    pub batch_id: Option<String>,
    pub name: String,
    pub task: Option<String>,
    pub status: SubagentStatus,
    pub allowed_tools: Vec<String>,
    pub tool_calls: Vec<SubagentToolCall>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentBatch {
    pub id: String,
    pub name: String,
    pub status: SubagentStatus,
    pub allowed_tools: Vec<String>,
    pub children: Vec<SubagentChild>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubagentTree {
    pub batches: Vec<SubagentBatch>,
    pub loose_children: Vec<SubagentChild>,
}

impl SubagentTree {
    pub fn upsert_batch(&mut self, batch: SubagentBatch) {
        if let Some(existing) = self.batches.iter_mut().find(|item| item.id == batch.id) {
            merge_batch(existing, batch);
            return;
        }
        self.batches.push(batch);
    }

    pub fn upsert_child(&mut self, child: SubagentChild) {
        if let Some(batch_id) = &child.batch_id {
            if let Some(batch) = self.batches.iter_mut().find(|batch| &batch.id == batch_id) {
                upsert_child_into(&mut batch.children, child);
                return;
            }
        }

        if let Some(index) = self
            .loose_children
            .iter()
            .position(|item| item.id == child.id)
        {
            let mut merged = self.loose_children.remove(index);
            merge_child(&mut merged, child);
            if let Some(batch_id) = &merged.batch_id {
                if let Some(batch) = self.batches.iter_mut().find(|batch| &batch.id == batch_id) {
                    upsert_child_into(&mut batch.children, merged);
                    return;
                }
            }
            self.loose_children.push(merged);
            return;
        }

        self.loose_children.push(child);
    }
}

fn merge_batch(existing: &mut SubagentBatch, incoming: SubagentBatch) {
    if !incoming.name.is_empty() {
        existing.name = incoming.name;
    }
    if incoming.status != SubagentStatus::Unknown {
        existing.status = incoming.status;
    }
    merge_unique(&mut existing.allowed_tools, incoming.allowed_tools);
    for child in incoming.children {
        upsert_child_into(&mut existing.children, child);
    }
}

fn upsert_child_into(children: &mut Vec<SubagentChild>, child: SubagentChild) {
    if let Some(existing) = children.iter_mut().find(|item| item.id == child.id) {
        merge_child(existing, child);
        return;
    }
    children.push(child);
}

fn merge_child(existing: &mut SubagentChild, incoming: SubagentChild) {
    if incoming.batch_id.is_some() {
        existing.batch_id = incoming.batch_id;
    }
    if !incoming.name.is_empty() {
        existing.name = incoming.name;
    }
    if incoming.task.is_some() {
        existing.task = incoming.task;
    }
    if incoming.status != SubagentStatus::Unknown {
        existing.status = incoming.status;
    }
    merge_unique(&mut existing.allowed_tools, incoming.allowed_tools);
    for call in incoming.tool_calls {
        if let Some(existing_call) = existing
            .tool_calls
            .iter_mut()
            .find(|item| item.id == call.id)
        {
            if !call.name.is_empty() {
                existing_call.name = call.name;
            }
            if call.status != SubagentStatus::Unknown {
                existing_call.status = call.status;
            }
            if call.detail.is_some() {
                existing_call.detail = call.detail;
            }
        } else {
            existing.tool_calls.push(call);
        }
    }
}

fn merge_unique(target: &mut Vec<String>, incoming: Vec<String>) {
    for item in incoming {
        if !target.iter().any(|existing| existing == &item) {
            target.push(item);
        }
    }
}
