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
            Self::Unknown => "pending",
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
            .or_else(|| {
                value.get("state").and_then(|state| match state {
                    Value::Object(_) => state.get("status").or_else(|| state.get("state")),
                    other => Some(other),
                })
            })
            .or_else(|| {
                value.get("result").and_then(|result| match result {
                    Value::Object(_) => result.get("status").or_else(|| result.get("state")),
                    _ => None,
                })
            })
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
        if event_type.ends_with(".start")
            || event_type.ends_with(".started")
            || event_type.ends_with(".progress")
            || event_type.ends_with(".output")
        {
            Self::Running
        } else if event_type.ends_with(".end")
            || event_type.ends_with(".ended")
            || event_type.ends_with(".completed")
            || event_type.ends_with(".executed")
            || event_type.ends_with(".succeeded")
            || event_type.ends_with(".persisted")
            || event_type.ends_with(".exit")
        {
            Self::Completed
        } else if event_type.ends_with(".failed") || event_type.ends_with(".error") {
            Self::Failed
        } else if event_type.ends_with(".ask_required") || event_type.ends_with(".budget.exhausted")
        {
            Self::NeedsUser
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
    pub call_id: Option<String>,
    pub job_id: Option<String>,
    pub child_id: Option<String>,
    pub server: Option<String>,
    pub tool: Option<String>,
    pub command: Option<String>,
    pub args_preview: Option<String>,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<u64>,
    pub detail: Option<String>,
    pub output_tail: Vec<String>,
    pub processes: Vec<SubagentProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentProcess {
    pub id: String,
    pub status: SubagentStatus,
    pub call_id: Option<String>,
    pub job_id: Option<String>,
    pub child_id: Option<String>,
    pub command: Option<String>,
    pub output_preview: Option<String>,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<u64>,
    pub output_tail: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAllocation {
    pub id: String,
    pub request_id: Option<String>,
    pub job_id: Option<String>,
    pub child_id: Option<String>,
    pub scope: Option<String>,
    pub agent_role: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub reason: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentAskPause {
    pub id: String,
    pub status: SubagentStatus,
    pub reason: Option<String>,
    pub crystal_candidate: Option<String>,
    pub answered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentChild {
    pub id: String,
    pub batch_id: Option<String>,
    pub job_id: Option<String>,
    pub name: String,
    pub task: Option<String>,
    pub status: SubagentStatus,
    pub limited: bool,
    pub limit_reason: Option<String>,
    pub suppressed_ask_required: bool,
    pub model: Option<ModelAllocation>,
    pub allowed_tools: Vec<String>,
    pub tool_calls: Vec<SubagentToolCall>,
    pub processes: Vec<SubagentProcess>,
    pub ask: Option<SubagentAskPause>,
    pub crystal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentBatch {
    pub id: String,
    pub job_id: Option<String>,
    pub name: String,
    pub status: SubagentStatus,
    pub allowed_tools: Vec<String>,
    pub children: Vec<SubagentChild>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubagentTree {
    pub batches: Vec<SubagentBatch>,
    pub loose_children: Vec<SubagentChild>,
    pub models: Vec<ModelAllocation>,
    pub loose_tool_calls: Vec<SubagentToolCall>,
    pub loose_processes: Vec<SubagentProcess>,
    pub loose_asks: Vec<SubagentAskPause>,
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
        let mut child = child;
        let child_id = child.id.clone();
        if child.model.is_none() {
            child.model = self
                .models
                .iter()
                .find(|model| model.child_id.as_deref() == Some(child_id.as_str()))
                .cloned();
        }
        for call in take_matching(&mut self.loose_tool_calls, |call| {
            call.child_id.as_deref() == Some(child_id.as_str())
                || child
                    .job_id
                    .as_deref()
                    .is_some_and(|job_id| call.job_id.as_deref() == Some(job_id))
        }) {
            upsert_tool_into(&mut child.tool_calls, call);
        }
        for process in take_matching(&mut self.loose_processes, |process| {
            process.child_id.as_deref() == Some(child_id.as_str())
                || child
                    .job_id
                    .as_deref()
                    .is_some_and(|job_id| process.job_id.as_deref() == Some(job_id))
        }) {
            upsert_process_on_child(&mut child, process);
        }

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

        if let Some(job_id) = child.job_id.as_deref() {
            if let Some(index) = self
                .loose_children
                .iter()
                .position(|item| item.job_id.as_deref() == Some(job_id))
            {
                let mut merged = self.loose_children.remove(index);
                merge_child(&mut merged, child);
                if let Some(batch_id) = &merged.batch_id {
                    if let Some(batch) = self.batches.iter_mut().find(|batch| &batch.id == batch_id)
                    {
                        upsert_child_into(&mut batch.children, merged);
                        return;
                    }
                }
                self.loose_children.push(merged);
                return;
            }
        }

        self.loose_children.push(child);
    }

    pub fn upsert_model(&mut self, model: ModelAllocation) {
        if let Some(child_id) = &model.child_id {
            if let Some(child) = self.child_mut(child_id) {
                child.model = Some(merge_model(child.model.take(), model.clone()));
            }
        }

        if let Some(existing) = self.models.iter_mut().find(|item| item.id == model.id) {
            *existing = merge_model(Some(existing.clone()), model);
        } else {
            self.models.push(model);
        }
    }

    pub fn upsert_tool_call(&mut self, call: SubagentToolCall) {
        let mut call = call;
        let call_id = call.call_id.clone().unwrap_or_else(|| call.id.clone());
        for process in take_matching(&mut self.loose_processes, |process| {
            process.call_id.as_deref() == Some(call_id.as_str())
        }) {
            upsert_process_into(&mut call.processes, process);
        }

        if let Some(child_id) = &call.child_id {
            if let Some(child) = self.child_mut(child_id) {
                upsert_tool_into(&mut child.tool_calls, call);
                return;
            }

            self.upsert_child(SubagentChild {
                id: child_id.clone(),
                batch_id: None,
                job_id: call.job_id.clone(),
                name: child_id.clone(),
                task: None,
                status: SubagentStatus::Unknown,
                limited: false,
                limit_reason: None,
                suppressed_ask_required: false,
                model: None,
                allowed_tools: Vec::new(),
                tool_calls: vec![call],
                processes: Vec::new(),
                ask: None,
                crystal: None,
            });
            return;
        }

        if let Some(job_id) = &call.job_id {
            if let Some(child) = self.child_mut_by_job(job_id) {
                upsert_tool_into(&mut child.tool_calls, call);
                return;
            }
        }

        upsert_tool_into(&mut self.loose_tool_calls, call);
    }

    pub fn upsert_process(&mut self, process: SubagentProcess) {
        if let Some(child_id) = &process.child_id {
            if let Some(child) = self.child_mut(child_id) {
                upsert_process_on_child(child, process);
                return;
            }
        }

        if let Some(job_id) = &process.job_id {
            if let Some(child) = self.child_mut_by_job(job_id) {
                upsert_process_on_child(child, process);
                return;
            }
        }

        if let Some(call_id) = &process.call_id {
            for call in &mut self.loose_tool_calls {
                if call.id == *call_id || call.call_id.as_deref() == Some(call_id.as_str()) {
                    upsert_process_into(&mut call.processes, process);
                    return;
                }
            }
        }

        upsert_process_into(&mut self.loose_processes, process);
    }

    pub fn upsert_ask(&mut self, ask: SubagentAskPause, child_id: Option<String>) {
        if let Some(child_id) = child_id {
            if let Some(child) = self.child_mut(&child_id) {
                merge_ask_on_child(child, ask);
                return;
            }
        }

        if let Some(existing) = self.loose_asks.iter_mut().find(|item| item.id == ask.id) {
            merge_ask(existing, ask);
        } else {
            self.loose_asks.push(ask);
        }
    }

    fn child_mut(&mut self, child_id: &str) -> Option<&mut SubagentChild> {
        for batch in &mut self.batches {
            if let Some(child) = batch.children.iter_mut().find(|child| child.id == child_id) {
                return Some(child);
            }
        }
        self.loose_children
            .iter_mut()
            .find(|child| child.id == child_id)
    }

    fn child_mut_by_job(&mut self, job_id: &str) -> Option<&mut SubagentChild> {
        for batch in &mut self.batches {
            if let Some(child) = batch
                .children
                .iter_mut()
                .find(|child| child.job_id.as_deref() == Some(job_id))
            {
                return Some(child);
            }
        }
        self.loose_children
            .iter_mut()
            .find(|child| child.job_id.as_deref() == Some(job_id))
    }
}

fn merge_batch(existing: &mut SubagentBatch, incoming: SubagentBatch) {
    if incoming.job_id.is_some() {
        existing.job_id = incoming.job_id;
    }
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
    if incoming.job_id.is_some() {
        existing.job_id = incoming.job_id;
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
    existing.limited |= incoming.limited;
    existing.suppressed_ask_required |= incoming.suppressed_ask_required;
    if incoming.limit_reason.is_some() {
        existing.limit_reason = incoming.limit_reason;
    }
    if incoming.model.is_some() {
        existing.model = incoming.model;
    }
    if incoming.ask.is_some() {
        existing.ask = incoming.ask;
    }
    if incoming.crystal.is_some() {
        existing.crystal = incoming.crystal;
    }
    merge_unique(&mut existing.allowed_tools, incoming.allowed_tools);
    for call in incoming.tool_calls {
        upsert_tool_into(&mut existing.tool_calls, call);
    }
    for process in incoming.processes {
        upsert_process_on_child(existing, process);
    }
}

fn upsert_tool_into(calls: &mut Vec<SubagentToolCall>, call: SubagentToolCall) {
    if let Some(existing) = calls.iter_mut().find(|item| {
        item.id == call.id
            || item.call_id.as_deref() == Some(call.id.as_str())
            || call.call_id.as_deref() == Some(item.id.as_str())
    }) {
        merge_tool(existing, call);
    } else {
        calls.push(call);
    }
}

fn merge_tool(existing: &mut SubagentToolCall, incoming: SubagentToolCall) {
    if !incoming.name.is_empty() {
        existing.name = incoming.name;
    }
    if incoming.status != SubagentStatus::Unknown {
        existing.status = incoming.status;
    }
    if incoming.call_id.is_some() {
        existing.call_id = incoming.call_id;
    }
    if incoming.job_id.is_some() {
        existing.job_id = incoming.job_id;
    }
    if incoming.child_id.is_some() {
        existing.child_id = incoming.child_id;
    }
    if incoming.server.is_some() {
        existing.server = incoming.server;
    }
    if incoming.tool.is_some() {
        existing.tool = incoming.tool;
    }
    if incoming.command.is_some() {
        existing.command = incoming.command;
    }
    if incoming.args_preview.is_some() {
        existing.args_preview = incoming.args_preview;
    }
    if incoming.output_path.is_some() {
        existing.output_path = incoming.output_path;
    }
    if incoming.error.is_some() {
        existing.error = incoming.error;
    }
    if incoming.duration_ms.is_some() {
        existing.duration_ms = incoming.duration_ms;
    }
    if incoming.detail.is_some() {
        existing.detail = incoming.detail;
    }
    if !incoming.output_tail.is_empty() {
        existing.output_tail = incoming.output_tail;
    }
    for process in incoming.processes {
        upsert_process_into(&mut existing.processes, process);
    }
}

fn upsert_process_on_child(child: &mut SubagentChild, process: SubagentProcess) {
    if let Some(call_id) = &process.call_id {
        if let Some(call) = child
            .tool_calls
            .iter_mut()
            .find(|call| call.id == *call_id || call.call_id.as_deref() == Some(call_id.as_str()))
        {
            upsert_process_into(&mut call.processes, process);
            return;
        }
    }
    upsert_process_into(&mut child.processes, process);
}

fn upsert_process_into(processes: &mut Vec<SubagentProcess>, process: SubagentProcess) {
    if let Some(existing) = processes.iter_mut().find(|item| item.id == process.id) {
        merge_process(existing, process);
    } else {
        processes.push(process);
    }
}

fn merge_process(existing: &mut SubagentProcess, incoming: SubagentProcess) {
    if incoming.status != SubagentStatus::Unknown {
        existing.status = incoming.status;
    }
    if incoming.call_id.is_some() {
        existing.call_id = incoming.call_id;
    }
    if incoming.job_id.is_some() {
        existing.job_id = incoming.job_id;
    }
    if incoming.child_id.is_some() {
        existing.child_id = incoming.child_id;
    }
    if incoming.command.is_some() {
        existing.command = incoming.command;
    }
    if incoming.output_preview.is_some() {
        existing.output_preview = incoming.output_preview;
    }
    if incoming.output_path.is_some() {
        existing.output_path = incoming.output_path;
    }
    if incoming.error.is_some() {
        existing.error = incoming.error;
    }
    if incoming.duration_ms.is_some() {
        existing.duration_ms = incoming.duration_ms;
    }
    if !incoming.output_tail.is_empty() {
        existing.output_tail = incoming.output_tail;
    }
}

fn merge_model(existing: Option<ModelAllocation>, incoming: ModelAllocation) -> ModelAllocation {
    let Some(mut existing) = existing else {
        return incoming;
    };
    if incoming.request_id.is_some() {
        existing.request_id = incoming.request_id;
    }
    if incoming.job_id.is_some() {
        existing.job_id = incoming.job_id;
    }
    if incoming.child_id.is_some() {
        existing.child_id = incoming.child_id;
    }
    if incoming.scope.is_some() {
        existing.scope = incoming.scope;
    }
    if incoming.agent_role.is_some() {
        existing.agent_role = incoming.agent_role;
    }
    if incoming.provider_id.is_some() {
        existing.provider_id = incoming.provider_id;
    }
    if incoming.model_id.is_some() {
        existing.model_id = incoming.model_id;
    }
    if incoming.reason.is_some() {
        existing.reason = incoming.reason;
    }
    if incoming.source.is_some() {
        existing.source = incoming.source;
    }
    existing
}

fn merge_ask_on_child(child: &mut SubagentChild, ask: SubagentAskPause) {
    if let Some(existing) = &mut child.ask {
        merge_ask(existing, ask);
    } else {
        child.ask = Some(ask);
    }
    child.status = SubagentStatus::NeedsUser;
}

fn merge_ask(existing: &mut SubagentAskPause, incoming: SubagentAskPause) {
    if incoming.status != SubagentStatus::Unknown {
        existing.status = incoming.status;
    }
    if incoming.reason.is_some() {
        existing.reason = incoming.reason;
    }
    if incoming.crystal_candidate.is_some() {
        existing.crystal_candidate = incoming.crystal_candidate;
    }
    existing.answered |= incoming.answered;
}

fn merge_unique(target: &mut Vec<String>, incoming: Vec<String>) {
    for item in incoming {
        if !target.iter().any(|existing| existing == &item) {
            target.push(item);
        }
    }
}

fn take_matching<T>(items: &mut Vec<T>, mut matches: impl FnMut(&T) -> bool) -> Vec<T> {
    let mut selected = Vec::new();
    let mut index = 0;
    while index < items.len() {
        if matches(&items[index]) {
            selected.push(items.remove(index));
        } else {
            index += 1;
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_status_is_not_rendered_as_unknown() {
        assert_eq!(SubagentStatus::Unknown.as_str(), "pending");
    }
}
