use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::tui::subagent::state::{
    ModelAllocation, SubagentAskPause, SubagentChild, SubagentProcess, SubagentStatus,
    SubagentToolCall, SubagentTree,
};

pub fn subagent_tree_lines(tree: &SubagentTree) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let child_count = subagent_child_count(tree);
    if child_count == 0 {
        return lines;
    }

    lines.push(Line::styled(
        format!("Subagents {child_count}"),
        Style::default()
            .fg(Color::Rgb(230, 236, 255))
            .add_modifier(Modifier::BOLD),
    ));
    for batch in &tree.batches {
        for child in &batch.children {
            lines.push(child_summary_line(child, Some(batch.name.as_str())));
        }
    }
    for child in &tree.loose_children {
        lines.push(child_summary_line(child, None));
    }
    lines
}

pub fn subagent_child_count(tree: &SubagentTree) -> usize {
    tree.batches
        .iter()
        .map(|batch| batch.children.len())
        .sum::<usize>()
        + tree.loose_children.len()
}

pub fn current_subagent(tree: &SubagentTree) -> Option<(Option<&str>, &SubagentChild)> {
    tree.batches
        .iter()
        .flat_map(|batch| {
            batch
                .children
                .iter()
                .map(|child| (Some(batch.name.as_str()), child))
        })
        .chain(tree.loose_children.iter().map(|child| (None, child)))
        .max_by_key(|(_, child)| status_rank(&child.status))
}

pub fn child_summary_line(child: &SubagentChild, batch_name: Option<&str>) -> Line<'static> {
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
            .map(|call| call.processes.len())
            .sum::<usize>();
    Line::from(vec![
        Span::styled(status_marker(&child.status), marker_style(&child.status)),
        Span::raw(" "),
        Span::styled("Task", Style::default().fg(Color::Rgb(126, 139, 170))),
        Span::raw(" "),
        Span::styled(
            truncate(task, 72),
            Style::default().fg(Color::Rgb(230, 236, 255)),
        ),
        Span::styled(
            batch_name
                .map(|name| format!(" · {name}"))
                .unwrap_or_default(),
            Style::default().fg(Color::Rgb(126, 139, 170)),
        ),
        Span::raw(" "),
        Span::styled(child.status.as_str(), marker_style(&child.status)),
        Span::styled(
            if child.limited { " · partial" } else { "" },
            Style::default().fg(Color::Rgb(255, 204, 102)),
        ),
        Span::styled(
            format!(" · tools {tool_count} · proc {process_count}"),
            Style::default().fg(Color::Rgb(126, 139, 170)),
        ),
    ])
}

pub fn child_detail_lines(
    child: &SubagentChild,
    batch_name: Option<&str>,
    indent: &str,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(batch_name) = batch_name {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}batch ")),
            Span::styled(
                truncate(batch_name, 80),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    lines.push(Line::from(vec![
        Span::raw(format!("{indent}id ")),
        Span::styled(
            child.id.clone(),
            Style::default().fg(Color::Rgb(126, 139, 170)),
        ),
    ]));
    if let Some(task) = &child.task {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}Task ")),
            Span::styled(
                truncate(task, 120),
                Style::default().fg(Color::Rgb(230, 236, 255)),
            ),
        ]));
    }
    if let Some(model) = &child.model {
        lines.extend(model_lines(model, indent));
    }
    if child.limited || child.suppressed_ask_required {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}partial ")),
            Span::styled(
                truncate(
                    child.limit_reason.as_deref().unwrap_or("partial-result"),
                    120,
                ),
                Style::default().fg(Color::Rgb(255, 204, 102)),
            ),
            Span::styled(
                if child.suppressed_ask_required {
                    " · ASK suppressed"
                } else {
                    ""
                },
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    if !child.allowed_tools.is_empty() {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}allowed tools ")),
            Span::styled(
                truncate(&child.allowed_tools.join(", "), 120),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    for call in &child.tool_calls {
        lines.extend(tool_lines(call, indent));
    }
    for process in &child.processes {
        lines.extend(process_lines(process, indent));
    }
    if let Some(ask) = &child.ask {
        lines.extend(ask_lines(ask, indent));
    }
    if let Some(crystal) = &child.crystal {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}crystal ")),
            Span::styled(
                truncate(crystal, 120),
                Style::default().fg(Color::Rgb(202, 188, 255)),
            ),
        ]));
    }
    lines
}

pub fn model_lines(model: &ModelAllocation, indent: &str) -> Vec<Line<'static>> {
    let model_name = match (&model.provider_id, &model.model_id) {
        (Some(provider), Some(model_id)) => format!("{provider}/{model_id}"),
        (None, Some(model_id)) => model_id.clone(),
        (Some(provider), None) => provider.clone(),
        (None, None) => model.id.clone(),
    };
    let mut lines = vec![Line::from(vec![
        Span::raw(format!("{indent}model ")),
        Span::styled(
            truncate(&model_name, 80),
            Style::default().fg(Color::Rgb(230, 236, 255)),
        ),
        Span::raw(" "),
        Span::styled(
            model
                .scope
                .clone()
                .unwrap_or_else(|| "selected".to_string()),
            Style::default().fg(Color::Rgb(126, 139, 170)),
        ),
    ])];
    if model.reason.is_some() || model.agent_role.is_some() {
        let detail = [model.agent_role.as_deref(), model.reason.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}  ")),
            Span::styled(
                truncate(&detail, 120),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    lines
}

fn tool_lines(call: &SubagentToolCall, indent: &str) -> Vec<Line<'static>> {
    let title = tool_title(call);
    let mut lines = vec![Line::from(vec![
        Span::raw(indent.to_string()),
        Span::styled(
            truncate(&title, 90),
            Style::default().fg(Color::Rgb(230, 236, 255)),
        ),
        Span::raw(" "),
        Span::styled(call.status.as_str(), marker_style(&call.status)),
    ])];
    let detail = [
        call.args_preview.as_deref(),
        call.command.as_deref(),
        call.output_path.as_deref(),
        call.detail.as_deref(),
    ]
    .into_iter()
    .flatten()
    .next();
    if let Some(detail) = detail {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}  result ")),
            Span::styled(
                truncate(detail, 120),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    if let Some(error) = &call.error {
        lines.push(error_line(indent, error));
    }
    if let Some(duration_ms) = call.duration_ms {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}  duration ")),
            Span::styled(
                format!("{duration_ms}ms"),
                Style::default().fg(Color::Rgb(126, 139, 170)),
            ),
        ]));
    }
    for process in &call.processes {
        lines.extend(process_lines(process, &format!("{indent}  ")));
    }
    lines
}

fn process_lines(process: &SubagentProcess, indent: &str) -> Vec<Line<'static>> {
    let title = process
        .command
        .clone()
        .or_else(|| process.output_path.clone())
        .unwrap_or_else(|| process.id.clone());
    let mut lines = vec![Line::from(vec![
        Span::raw(format!("{indent}process ")),
        Span::styled(
            truncate(&title, 90),
            Style::default().fg(Color::Rgb(230, 236, 255)),
        ),
        Span::raw(" "),
        Span::styled(process.status.as_str(), marker_style(&process.status)),
    ])];
    let detail = [
        process.output_preview.as_deref(),
        process.output_path.as_deref(),
        process.error.as_deref(),
    ]
    .into_iter()
    .flatten()
    .next();
    if let Some(detail) = detail {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}  result ")),
            Span::styled(
                truncate(detail, 120),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    if let Some(error) = &process.error {
        lines.push(error_line(indent, error));
    }
    lines
}

fn ask_lines(ask: &SubagentAskPause, indent: &str) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::raw(format!("{indent}ASK ")),
        Span::styled(
            ask.id.clone(),
            Style::default().fg(Color::Rgb(230, 236, 255)),
        ),
        Span::raw(" "),
        Span::styled(ask.status.as_str(), marker_style(&ask.status)),
    ])];
    if let Some(reason) = &ask.reason {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}  ")),
            Span::styled(
                truncate(reason, 120),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    if let Some(crystal) = &ask.crystal_candidate {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}  crystal ")),
            Span::styled(
                truncate(crystal, 120),
                Style::default().fg(Color::Rgb(202, 188, 255)),
            ),
        ]));
    }
    lines
}

fn status_marker(status: &SubagentStatus) -> &'static str {
    match status {
        SubagentStatus::Pending => "○",
        SubagentStatus::Running => "◆",
        SubagentStatus::NeedsUser => "!",
        SubagentStatus::Completed => "✓",
        SubagentStatus::Failed => "×",
        SubagentStatus::Unknown => "•",
    }
}

fn marker_style(status: &SubagentStatus) -> Style {
    let color = match status {
        SubagentStatus::Pending => Color::Rgb(126, 139, 170),
        SubagentStatus::Running => Color::Rgb(111, 192, 255),
        SubagentStatus::NeedsUser => Color::Rgb(255, 204, 102),
        SubagentStatus::Completed => Color::Rgb(116, 214, 148),
        SubagentStatus::Failed => Color::Rgb(255, 105, 130),
        SubagentStatus::Unknown => Color::Rgb(166, 142, 255),
    };
    Style::default().fg(color)
}

fn status_rank(status: &SubagentStatus) -> usize {
    match status {
        SubagentStatus::Running => 5,
        SubagentStatus::NeedsUser => 4,
        SubagentStatus::Failed => 3,
        SubagentStatus::Pending => 2,
        SubagentStatus::Completed => 1,
        SubagentStatus::Unknown => 0,
    }
}

fn tool_title(call: &SubagentToolCall) -> String {
    let name = call
        .tool
        .as_deref()
        .or(call.command.as_deref())
        .unwrap_or(call.name.as_str());
    let target = call
        .args_preview
        .as_deref()
        .or(call.output_path.as_deref())
        .or(call.detail.as_deref())
        .unwrap_or_default();
    let canonical = canonical_tool_name(name);
    if target.is_empty() {
        return canonical.to_string();
    }
    format!("{canonical} {}", truncate(target, 72))
}

fn canonical_tool_name(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    match lower.rsplit(['/', '.']).next().unwrap_or(lower.as_str()) {
        "task" => "Task",
        "read" => "Read",
        "glob" => "Glob",
        "tree" | "list" => "Tree",
        "grep" => "Grep",
        "write" => "Write",
        "edit" => "Edit",
        "bash" | "shell" | "exec" => "Shell",
        _ => "Tool",
    }
}

fn error_line(indent: &str, error: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!("{indent}  error ")),
        Span::styled(
            truncate(error, 120),
            Style::default().fg(Color::Rgb(255, 105, 130)),
        ),
    ])
}

pub fn truncate(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let mut out = String::new();
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

    use crate::tui::subagent::parser::merge_execution_job_snapshot;

    use super::*;

    #[test]
    fn renders_needs_user_state() {
        let mut tree = SubagentTree::default();
        merge_execution_job_snapshot(
            &mut tree,
            &json!({
                "data": {
                    "children": [{
                        "id": "child-1",
                        "status": "needs_user",
                        "task": "Pick an option",
                        "allowedTools": ["read"],
                        "toolCalls": [{ "id": "call-1", "name": "read", "status": "completed" }]
                    }]
                }
            }),
        );

        let summary = subagent_tree_lines(&tree)
            .into_iter()
            .map(|line| {
                line.spans
                    .into_iter()
                    .map(|span| span.content.into_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let child = &tree.loose_children[0];
        let detail = child_detail_lines(child, None, "")
            .into_iter()
            .map(|line| {
                line.spans
                    .into_iter()
                    .map(|span| span.content.into_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(summary.contains("Subagents 1"));
        assert!(summary.contains("Task Pick an option"));
        assert!(summary.contains("needs_user"));
        assert!(detail.contains("allowed tools read"));
        assert!(detail.contains("Read"));
        assert!(!summary.contains("allowed tools read"));
    }

    #[test]
    fn renders_limited_partial_child_state() {
        let mut tree = SubagentTree::default();
        merge_execution_job_snapshot(
            &mut tree,
            &json!({
                "data": {
                    "children": [{
                        "id": "reader",
                        "status": "completed",
                        "limited": true,
                        "limitReason": "tool-budget-exhausted",
                        "suppressedAskRequired": true
                    }]
                }
            }),
        );

        let summary = subagent_tree_lines(&tree)
            .into_iter()
            .map(|line| {
                line.spans
                    .into_iter()
                    .map(|span| span.content.into_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let child = &tree.loose_children[0];
        let detail = child_detail_lines(child, None, "")
            .into_iter()
            .map(|line| {
                line.spans
                    .into_iter()
                    .map(|span| span.content.into_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(summary.contains("partial"));
        assert!(detail.contains("tool-budget-exhausted"));
        assert!(detail.contains("ASK suppressed"));
    }
}
