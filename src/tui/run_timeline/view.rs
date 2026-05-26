use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::tui::{
    run_timeline::state::{
        RunTimeline, RunTimelineItem, RunTimelineItemKind, RunTimelineItemStatus,
    },
    subagent::view::subagent_tree_lines,
};

pub fn run_panel_lines(timeline: &RunTimeline) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        "Run",
        Style::default()
            .fg(Color::Rgb(230, 236, 255))
            .add_modifier(Modifier::BOLD),
    )];

    if timeline.items.is_empty() && timeline.subagents.batches.is_empty() {
        lines.push(Line::styled(
            "waiting for run events",
            Style::default().fg(Color::Rgb(126, 139, 170)),
        ));
        return lines;
    }

    for item in &timeline.items {
        lines.extend(item_lines(item));
    }

    let tree_lines = subagent_tree_lines(&timeline.subagents);
    if !tree_lines.is_empty() {
        lines.push(Line::raw(""));
        lines.extend(tree_lines);
    }

    lines
}

pub fn run_panel<'a>(timeline: &RunTimeline) -> Paragraph<'a> {
    Paragraph::new(run_panel_lines(timeline))
        .block(Block::default().title(" Run ").borders(Borders::ALL))
        .wrap(Wrap { trim: true })
}

fn item_lines(item: &RunTimelineItem) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(status_marker(&item.status), marker_style(&item.status)),
        Span::raw(" "),
        Span::styled(
            kind_label(&item.kind),
            Style::default().fg(Color::Rgb(126, 139, 170)),
        ),
        Span::raw(" "),
        Span::styled(
            item.title.clone(),
            Style::default().fg(Color::Rgb(230, 236, 255)),
        ),
    ])];
    if let Some(detail) = &item.detail {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                detail.clone(),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    lines
}

fn status_marker(status: &RunTimelineItemStatus) -> &'static str {
    match status {
        RunTimelineItemStatus::Pending => "○",
        RunTimelineItemStatus::Running => "●",
        RunTimelineItemStatus::NeedsUser => "!",
        RunTimelineItemStatus::Completed => "✓",
        RunTimelineItemStatus::Failed => "×",
        RunTimelineItemStatus::Info => "•",
    }
}

fn marker_style(status: &RunTimelineItemStatus) -> Style {
    let color = match status {
        RunTimelineItemStatus::Pending => Color::Rgb(126, 139, 170),
        RunTimelineItemStatus::Running => Color::Rgb(111, 192, 255),
        RunTimelineItemStatus::NeedsUser => Color::Rgb(255, 204, 102),
        RunTimelineItemStatus::Completed => Color::Rgb(116, 214, 148),
        RunTimelineItemStatus::Failed => Color::Rgb(255, 105, 130),
        RunTimelineItemStatus::Info => Color::Rgb(166, 142, 255),
    };
    Style::default().fg(color)
}

fn kind_label(kind: &RunTimelineItemKind) -> &'static str {
    match kind {
        RunTimelineItemKind::Route => "route",
        RunTimelineItemKind::Recall => "recall",
        RunTimelineItemKind::Tool => "tool",
        RunTimelineItemKind::Blackboard => "blackboard",
        RunTimelineItemKind::Subagent => "subagent",
        RunTimelineItemKind::Ask => "ASK",
        RunTimelineItemKind::Plan => "plan",
        RunTimelineItemKind::Fork => "fork",
        RunTimelineItemKind::Loop => "loop",
        RunTimelineItemKind::Snapshot => "snapshot",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::tui::{
        run_timeline::state::RunTimeline, subagent::parser::merge_execution_job_snapshot,
    };

    use super::*;

    #[test]
    fn run_panel_shows_needs_user_subagent() {
        let mut timeline = RunTimeline::new();
        merge_execution_job_snapshot(
            &mut timeline.subagents,
            &json!({
                "data": {
                    "batches": [{ "id": "batch-1" }],
                    "children": [{
                        "id": "child-1",
                        "batchId": "batch-1",
                        "status": "needs_user",
                        "task": "Need approval"
                    }]
                }
            }),
        );

        let rendered = run_panel_lines(&timeline)
            .into_iter()
            .map(|line| {
                line.spans
                    .into_iter()
                    .map(|span| span.content.into_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("needs_user"));
        assert!(rendered.contains("Need approval"));
    }

    #[test]
    fn run_panel_widget_is_constructible() {
        let timeline = RunTimeline::new();
        let _widget = run_panel(&timeline);
    }
}
