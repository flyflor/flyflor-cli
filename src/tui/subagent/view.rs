use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::tui::subagent::state::{SubagentBatch, SubagentChild, SubagentStatus, SubagentTree};

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
            Span::styled(task.clone(), Style::default().fg(Color::Rgb(170, 180, 205))),
        ]));
    }
    if !child.allowed_tools.is_empty() {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}  allowed tools ")),
            Span::styled(
                child.allowed_tools.join(", "),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    for call in &child.tool_calls {
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}  tool ")),
            Span::styled(
                call.name.clone(),
                Style::default().fg(Color::Rgb(230, 236, 255)),
            ),
            Span::raw(" "),
            Span::styled(call.status.as_str(), marker_style(&call.status)),
        ]));
        if let Some(detail) = &call.detail {
            lines.push(Line::from(vec![
                Span::raw(format!("{indent}    ")),
                Span::styled(
                    detail.clone(),
                    Style::default().fg(Color::Rgb(170, 180, 205)),
                ),
            ]));
        }
    }
    lines
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
