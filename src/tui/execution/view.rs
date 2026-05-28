use crate::{
    i18n::{CopyKey, text, text_key},
    tui::{
        execution::state::{ExecutionContextRow, ExecutionRowStatus},
        run_timeline::state::{RunTimeline, RunTimelineItemStatus},
        subagent::state::{
            ModelAllocation, SubagentChild, SubagentProcess, SubagentStatus, SubagentToolCall,
        },
    },
};

pub fn execution_context_rows(timeline: &RunTimeline) -> Vec<ExecutionContextRow> {
    let mut children = Vec::new();
    for batch in &timeline.subagents.batches {
        for child in &batch.children {
            children.push((Some(batch.name.as_str()), child));
        }
    }
    for child in &timeline.subagents.loose_children {
        children.push((None, child));
    }

    if children.is_empty() {
        if timeline.items.is_empty() {
            return Vec::new();
        }
        return vec![ExecutionContextRow {
            summary: execution_context_summary(timeline),
            detail: execution_context_detail(timeline),
            status: timeline_status(timeline),
            expanded: true,
            identity: "execution:timeline".to_string(),
        }];
    }

    let total = children.len();
    children
        .into_iter()
        .enumerate()
        .map(|(index, (batch_name, child))| ExecutionContextRow {
            summary: subagent_execution_summary(index + 1, total, child),
            detail: subagent_execution_detail(batch_name, child),
            status: child_status(&child.status),
            expanded: index + 1 == total,
            identity: format!("child:{}", child.id),
        })
        .collect()
}

fn execution_context_summary(timeline: &RunTimeline) -> String {
    let model_count = timeline.subagents.models.len();
    let batch_count = timeline.subagents.batches.len();
    let child_count = child_count(timeline);
    let tool_count = tool_count(timeline);
    let process_count = process_count(timeline);
    let status = match timeline_status(timeline) {
        ExecutionRowStatus::NeedsUser => text(CopyKey::WaitingAsk),
        ExecutionRowStatus::Running | ExecutionRowStatus::Pending => text(CopyKey::Running),
        ExecutionRowStatus::Failed => text(CopyKey::Failed),
        ExecutionRowStatus::Completed => text(CopyKey::Recorded),
    };
    format!(
        "{status} · {} {model_count} · {} {batch_count}/{child_count} · {} {tool_count} · {} {process_count}",
        text(CopyKey::Models),
        text(CopyKey::Subagents),
        text(CopyKey::Tools),
        text(CopyKey::Processes),
    )
}

fn execution_context_detail(timeline: &RunTimeline) -> String {
    timeline
        .items
        .iter()
        .rev()
        .take(18)
        .rev()
        .map(|item| {
            item.detail
                .as_ref()
                .map(|detail| format!("{} · {}", item.title, one_line(detail, 120)))
                .unwrap_or_else(|| item.title.clone())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn subagent_execution_summary(index: usize, total: usize, child: &SubagentChild) -> String {
    let task = child
        .task
        .as_deref()
        .filter(|task| !task.trim().is_empty())
        .unwrap_or(child.name.as_str());
    let tool_count = child.tool_calls.len();
    let process_count = child.processes.len()
        + child
            .tool_calls
            .iter()
            .map(|tool| tool.processes.len())
            .sum::<usize>();
    let limited = if child.limited {
        format!(" · {}", text(CopyKey::Partial))
    } else {
        String::new()
    };
    format!(
        "({index}/{total}) {} | {} · {}{limited} · {} {tool_count} · {} {process_count}",
        one_line(&child.name, 24),
        one_line(task, 42),
        child.status.as_str(),
        text(CopyKey::Tools),
        text(CopyKey::Processes),
    )
}

fn subagent_execution_detail(batch_name: Option<&str>, child: &SubagentChild) -> String {
    let mut lines = vec![format!("{}: {}", text(CopyKey::ChildId), child.id)];
    if let Some(batch_name) = batch_name {
        lines.push(format!("{}: {batch_name}", text(CopyKey::Batch)));
    }
    lines.push(format!(
        "{}: {}",
        text(CopyKey::Status),
        child.status.as_str()
    ));
    if child.limited || child.suppressed_ask_required {
        let limit_reason = child
            .limit_reason
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| text_key("execution.partialResult"));
        lines.push(format!(
            "{}: {}{}",
            text(CopyKey::Limit),
            limit_reason,
            if child.suppressed_ask_required {
                format!(" · {}", text(CopyKey::AskSuppressed))
            } else {
                String::new()
            }
        ));
    }
    if let Some(job_id) = &child.job_id {
        lines.push(format!("{}: {job_id}", text_key("execution.jobId")));
    }
    if let Some(task) = &child.task {
        lines.push(format!("{}: {task}", text(CopyKey::Task)));
    }
    if let Some(model) = &child.model {
        lines.push(format!(
            "{}: {}",
            text(CopyKey::Model),
            model_allocation_label(model)
        ));
        if let Some(reason) = &model.reason {
            lines.push(format!("{}: {reason}", text(CopyKey::ModelAllocation)));
        }
    }
    if !child.allowed_tools.is_empty() {
        lines.push(format!(
            "{}: {}",
            text(CopyKey::AllowedTools),
            child.allowed_tools.join(", ")
        ));
    }
    if child.tool_calls.is_empty() && child.processes.is_empty() {
        lines.push(text(CopyKey::ExecutionEmpty).to_string());
    }
    for tool in &child.tool_calls {
        push_tool_lines(&mut lines, tool, "");
    }
    for process in &child.processes {
        push_process_lines(&mut lines, process, "");
    }
    if let Some(ask) = &child.ask {
        lines.push(format!(
            "{}: {} · {}",
            text_key("execution.ask"),
            ask.id,
            ask.status.as_str()
        ));
        if let Some(reason) = &ask.reason {
            lines.push(format!("{}: {reason}", text(CopyKey::AskReason)));
        }
        if let Some(crystal) = &ask.crystal_candidate {
            lines.push(format!("{}: {crystal}", text(CopyKey::CrystalCandidate)));
        }
    }
    if let Some(crystal) = &child.crystal {
        lines.push(format!("{}: {crystal}", text(CopyKey::Crystal)));
    }
    lines.join("\n")
}

fn push_tool_lines(lines: &mut Vec<String>, tool: &SubagentToolCall, indent: &str) {
    lines.push(format!(
        "{indent}- {} {} · {}",
        text_key("execution.tool"),
        subagent_tool_label(tool),
        tool.status.as_str()
    ));
    if let Some(args) = &tool.args_preview {
        lines.push(format!(
            "{indent}  {}: {}",
            text(CopyKey::Args),
            one_line(args, 120)
        ));
    }
    if let Some(output_path) = &tool.output_path {
        lines.push(format!(
            "{indent}  {}: {output_path}",
            text(CopyKey::Output)
        ));
    }
    for tail in tool
        .output_tail
        .iter()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        lines.push(format!("{indent}  > {}", one_line(tail, 120)));
    }
    if let Some(error) = &tool.error {
        lines.push(format!(
            "{indent}  {}: {}",
            text(CopyKey::Error),
            one_line(error, 120)
        ));
    }
    for process in &tool.processes {
        push_process_lines(lines, process, "  ");
    }
}

fn push_process_lines(lines: &mut Vec<String>, process: &SubagentProcess, indent: &str) {
    lines.push(format!(
        "{indent}- {} {} · {}",
        text_key("execution.process"),
        subagent_process_label(process),
        process.status.as_str()
    ));
    if let Some(output) = &process.output_preview {
        lines.push(format!(
            "{indent}  {}: {}",
            text(CopyKey::Result),
            one_line(output, 120)
        ));
    }
    for tail in process
        .output_tail
        .iter()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        lines.push(format!("{indent}  > {}", one_line(tail, 120)));
    }
    if let Some(error) = &process.error {
        lines.push(format!(
            "{indent}  {}: {}",
            text(CopyKey::Error),
            one_line(error, 120)
        ));
    }
}

fn model_allocation_label(model: &ModelAllocation) -> String {
    match (&model.provider_id, &model.model_id) {
        (Some(provider), Some(model_id)) => format!("{provider}/{model_id}"),
        (None, Some(model_id)) => model_id.clone(),
        (Some(provider), None) => provider.clone(),
        (None, None) => model.id.clone(),
    }
}

fn subagent_tool_label(tool: &SubagentToolCall) -> String {
    if let (Some(server), Some(tool_name)) = (&tool.server, &tool.tool) {
        return format!("{server}/{tool_name}");
    }
    tool.tool
        .clone()
        .or_else(|| tool.command.clone())
        .or_else(|| (!tool.name.trim().is_empty()).then(|| tool.name.clone()))
        .unwrap_or_else(|| text_key("execution.defaultTool"))
}

fn subagent_process_label(process: &SubagentProcess) -> String {
    process
        .command
        .clone()
        .or_else(|| process.output_path.clone())
        .unwrap_or_else(|| process.id.clone())
}

fn timeline_status(timeline: &RunTimeline) -> ExecutionRowStatus {
    if timeline
        .items
        .iter()
        .any(|item| item.status == RunTimelineItemStatus::NeedsUser)
    {
        return ExecutionRowStatus::NeedsUser;
    }
    if timeline
        .items
        .iter()
        .any(|item| item.status == RunTimelineItemStatus::Running)
    {
        return ExecutionRowStatus::Running;
    }
    if timeline
        .items
        .iter()
        .any(|item| item.status == RunTimelineItemStatus::Failed)
    {
        return ExecutionRowStatus::Failed;
    }
    ExecutionRowStatus::Completed
}

fn child_status(status: &SubagentStatus) -> ExecutionRowStatus {
    match status {
        SubagentStatus::Pending | SubagentStatus::Unknown => ExecutionRowStatus::Pending,
        SubagentStatus::Running => ExecutionRowStatus::Running,
        SubagentStatus::NeedsUser => ExecutionRowStatus::NeedsUser,
        SubagentStatus::Completed => ExecutionRowStatus::Completed,
        SubagentStatus::Failed => ExecutionRowStatus::Failed,
    }
}

fn child_count(timeline: &RunTimeline) -> usize {
    timeline
        .subagents
        .batches
        .iter()
        .map(|batch| batch.children.len())
        .sum::<usize>()
        + timeline.subagents.loose_children.len()
}

fn tool_count(timeline: &RunTimeline) -> usize {
    timeline.subagents.loose_tool_calls.len()
        + timeline
            .subagents
            .batches
            .iter()
            .map(|batch| {
                batch
                    .children
                    .iter()
                    .map(|child| child.tool_calls.len())
                    .sum::<usize>()
            })
            .sum::<usize>()
        + timeline
            .subagents
            .loose_children
            .iter()
            .map(|child| child.tool_calls.len())
            .sum::<usize>()
}

fn process_count(timeline: &RunTimeline) -> usize {
    timeline.subagents.loose_processes.len()
        + timeline
            .subagents
            .batches
            .iter()
            .map(|batch| {
                batch
                    .children
                    .iter()
                    .map(child_process_count)
                    .sum::<usize>()
            })
            .sum::<usize>()
        + timeline
            .subagents
            .loose_children
            .iter()
            .map(child_process_count)
            .sum::<usize>()
}

fn child_process_count(child: &SubagentChild) -> usize {
    child.processes.len()
        + child
            .tool_calls
            .iter()
            .map(|tool| tool.processes.len())
            .sum::<usize>()
}

fn one_line(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    let mut chars = compact.chars();
    for _ in 0..max_chars {
        let Some(ch) = chars.next() else {
            return out;
        };
        out.push(ch);
    }
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::tui::run_timeline::state::RunTimeline;

    use super::*;

    #[test]
    fn execution_rows_are_expanded_and_include_tail_lines() {
        let mut timeline = RunTimeline::new();
        timeline.apply_execution_job_snapshot(&json!({
            "data": {
                "children": [{
                    "childId": "reader",
                    "childJobId": "job-child",
                    "task": "inspect"
                }],
                "toolExecutions": [{
                    "id": "call-1",
                    "childJobId": "job-child",
                    "server": "workspace",
                    "tool": "read",
                    "status": "completed",
                    "resultTailLines": ["a", "b", "c", "d"]
                }]
            }
        }));

        let rows = execution_context_rows(&timeline);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].expanded);
        assert!(rows[0].detail.contains("workspace/read"));
        assert!(!rows[0].detail.contains("tool · unknown"));
        assert!(rows[0].detail.contains("> b"));
        assert!(rows[0].detail.contains("> d"));
    }

    #[test]
    fn execution_rows_expand_only_latest_child_by_default() {
        let mut timeline = RunTimeline::new();
        timeline.apply_execution_job_snapshot(&json!({
            "data": {
                "children": [
                    { "childId": "reader", "task": "inspect", "status": "completed" },
                    { "childId": "writer", "task": "edit", "status": "running" }
                ]
            }
        }));

        let rows = execution_context_rows(&timeline);

        assert_eq!(rows.len(), 2);
        assert!(!rows[0].expanded);
        assert!(rows[1].expanded);
    }
}
