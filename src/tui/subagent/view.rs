use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::tui::subagent::state::{
    ModelAllocation, SubagentAskPause, SubagentBatch, SubagentChild, SubagentProcess,
    SubagentStatus, SubagentToolCall, SubagentTree,
};

pub fn subagent_tree_lines(tree: &SubagentTree) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if tree.batches.is_empty() && tree.loose_children.is_empty() {
        return lines;
    }

    lines.push(Line::styled(
        "Subagents",
        Style::default()
            .fg(Color::Rgb(230, 236, 255))
            .add_modifier(Modifier::BOLD),
    ));
    for batch in &tree.batches {
        lines.extend(batch_lines(batch));
    }
    for child in &tree.loose_children {
        lines.extend(child_lines(child, false));
    }
    if !tree.loose_tool_calls.is_empty() {
        lines.push(section_line("Loose tools"));
        for call in &tree.loose_tool_calls {
            lines.extend(tool_lines(call, ""));
        }
    }
    if !tree.loose_processes.is_empty() {
        lines.push(section_line("Loose processes"));
        for process in &tree.loose_processes {
            lines.extend(process_lines(process, ""));
        }
    }
    if !tree.loose_asks.is_empty() {
        lines.push(section_line("ASK"));
        for ask in &tree.loose_asks {
            lines.extend(ask_lines(ask, ""));
        }
    }
    lines
}

fn batch_lines(batch: &SubagentBatch) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(status_marker(&batch.status), marker_style(&batch.status)),
        Span::raw(" batch "),
        Span::styled(
            batch.name.clone(),
            Style::default().fg(Color::Rgb(230, 236, 255)),
        ),
        Span::raw(" "),
        Span::styled(batch.status.as_str(), marker_style(&batch.status)),
    ])];
    if !batch.allowed_tools.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  allowed tools "),
            Span::styled(
                batch.allowed_tools.join(", "),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    for child in &batch.children {
        lines.extend(child_lines(child, true));
    }
    lines
}

fn child_lines(child: &SubagentChild, nested: bool) -> Vec<Line<'static>> {
    let indent = if nested { "  " } else { "" };
    let mut lines = vec![Line::from(vec![
        Span::raw(indent.to_string()),
        Span::styled(status_marker(&child.status), marker_style(&child.status)),
        Span::raw(" child "),
        Span::styled(
            child.name.clone(),
            Style::default().fg(Color::Rgb(230, 236, 255)),
        ),
        Span::raw(" "),
        Span::styled(child.status.as_str(), marker_style(&child.status)),
    ])];
    if let Some(task) = &child.task {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}  task ")),
            Span::styled(
                truncate(task, 120),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    if let Some(model) = &child.model {
        lines.extend(model_lines(model, &format!("{indent}  ")));
    }
    if !child.allowed_tools.is_empty() {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}  allowed tools ")),
            Span::styled(
                truncate(&child.allowed_tools.join(", "), 120),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    for call in &child.tool_calls {
        lines.extend(tool_lines(call, &format!("{indent}  ")));
    }
    for process in &child.processes {
        lines.extend(process_lines(process, &format!("{indent}  ")));
    }
    if let Some(ask) = &child.ask {
        lines.extend(ask_lines(ask, &format!("{indent}  ")));
    }
    if let Some(crystal) = &child.crystal {
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
    let mut title = call
        .tool
        .clone()
        .or_else(|| call.command.clone())
        .unwrap_or_else(|| call.name.clone());
    if let Some(server) = &call.server {
        title = format!("{server}/{title}");
    }
    let mut lines = vec![Line::from(vec![
        Span::raw(format!("{indent}tool ")),
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
        call.error.as_deref(),
        call.detail.as_deref(),
    ]
    .into_iter()
    .flatten()
    .next();
    if let Some(detail) = detail {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}  ")),
            Span::styled(
                truncate(detail, 120),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
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
            Span::raw(format!("{indent}  ")),
            Span::styled(
                truncate(detail, 120),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
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

fn section_line(title: &str) -> Line<'static> {
    Line::styled(
        title.to_string(),
        Style::default()
            .fg(Color::Rgb(230, 236, 255))
            .add_modifier(Modifier::BOLD),
    )
}

fn status_marker(status: &SubagentStatus) -> &'static str {
    match status {
        SubagentStatus::Pending => "○",
        SubagentStatus::Running => "●",
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

        let text = subagent_tree_lines(&tree)
            .into_iter()
            .map(|line| {
                line.spans
                    .into_iter()
                    .map(|span| span.content.into_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("needs_user"));
        assert!(text.contains("Pick an option"));
        assert!(text.contains("allowed tools read"));
        assert!(text.contains("tool read completed"));
    }
}
