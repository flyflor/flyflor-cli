use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::{
    i18n::text_key,
    tui::{
        run_timeline::state::{
            RunTimeline, RunTimelineItem, RunTimelineItemKind, RunTimelineItemStatus,
        },
        subagent::view::{
            child_detail_lines, child_summary_line, current_subagent, subagent_child_count,
            truncate,
        },
    },
};

pub fn run_panel_lines(timeline: &RunTimeline) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        text_key("run.title"),
        Style::default()
            .fg(Color::Rgb(230, 236, 255))
            .add_modifier(Modifier::BOLD),
    )];

    if timeline.items.is_empty() && subagent_child_count(&timeline.subagents) == 0 {
        lines.push(Line::styled(
            text_key("run.waiting"),
            Style::default().fg(Color::Rgb(126, 139, 170)),
        ));
        return lines;
    }

    lines.push(summary_line(timeline));

    if let Some((batch_name, child)) = current_subagent(&timeline.subagents) {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            text_key("run.current"),
            Style::default()
                .fg(Color::Rgb(230, 236, 255))
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(child_summary_line(child, batch_name));
        lines.extend(child_detail_lines(child, batch_name, "  "));
    } else if let Some(item) = timeline.items.last() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            text_key("run.current"),
            Style::default()
                .fg(Color::Rgb(230, 236, 255))
                .add_modifier(Modifier::BOLD),
        ));
        lines.extend(item_lines(item));
    }

    lines
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
                truncate(detail, 120),
                Style::default().fg(Color::Rgb(170, 180, 205)),
            ),
        ]));
    }
    lines
}

fn summary_line(timeline: &RunTimeline) -> Line<'static> {
    let count = |kind: RunTimelineItemKind| {
        timeline
            .items
            .iter()
            .filter(|item| item.kind == kind)
            .count()
    };
    let tool_count = timeline.subagents.loose_tool_calls.len()
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
            .sum::<usize>();
    let process_count = timeline.subagents.loose_processes.len()
        + timeline
            .subagents
            .batches
            .iter()
            .map(|batch| {
                batch
                    .children
                    .iter()
                    .map(|child| {
                        child.processes.len()
                            + child
                                .tool_calls
                                .iter()
                                .map(|call| call.processes.len())
                                .sum::<usize>()
                    })
                    .sum::<usize>()
            })
            .sum::<usize>()
        + timeline
            .subagents
            .loose_children
            .iter()
            .map(|child| {
                child.processes.len()
                    + child
                        .tool_calls
                        .iter()
                        .map(|call| call.processes.len())
                        .sum::<usize>()
            })
            .sum::<usize>();
    let subagent_count = subagent_child_count(&timeline.subagents);
    let parts = [
        format!(
            "{} {}",
            text_key("run.kind.model"),
            timeline
                .subagents
                .models
                .len()
                .max(count(RunTimelineItemKind::Model))
        ),
        format!(
            "{} {}",
            text_key("run.kind.route"),
            count(RunTimelineItemKind::Route)
        ),
        format!(
            "{} {}",
            text_key("run.kind.blackboard"),
            count(RunTimelineItemKind::Blackboard)
        ),
        format!("{} {tool_count}", text_key("run.kind.tools")),
        format!("{} {subagent_count}", text_key("run.kind.subagents")),
        format!("{} {process_count}", text_key("run.kind.processes")),
        format!(
            "{} {}",
            text_key("run.kind.ask"),
            count(RunTimelineItemKind::Ask) + timeline.subagents.loose_asks.len()
        ),
        format!(
            "{} {}",
            text_key("run.kind.crystal"),
            count(RunTimelineItemKind::Crystal)
        ),
    ];
    Line::styled(
        parts.join(" · "),
        Style::default().fg(Color::Rgb(126, 139, 170)),
    )
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

fn kind_label(kind: &RunTimelineItemKind) -> String {
    match kind {
        RunTimelineItemKind::Model => text_key("run.kind.model"),
        RunTimelineItemKind::Route => text_key("run.kind.route"),
        RunTimelineItemKind::Recall => text_key("run.kind.recall"),
        RunTimelineItemKind::Tool => text_key("run.kind.tool"),
        RunTimelineItemKind::Process => text_key("run.kind.process"),
        RunTimelineItemKind::Blackboard => text_key("run.kind.blackboard"),
        RunTimelineItemKind::Subagent => text_key("run.kind.subagent"),
        RunTimelineItemKind::Ask => text_key("run.kind.ask"),
        RunTimelineItemKind::Confirm => text_key("run.kind.confirm"),
        RunTimelineItemKind::Plan => text_key("run.kind.plan"),
        RunTimelineItemKind::Fork => text_key("run.kind.fork"),
        RunTimelineItemKind::Loop => text_key("run.kind.loop"),
        RunTimelineItemKind::Snapshot => text_key("run.kind.snapshot"),
        RunTimelineItemKind::Crystal => text_key("run.kind.crystal"),
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
    fn run_panel_shows_waiting_without_events() {
        let timeline = RunTimeline::new();
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

        assert!(rendered.contains(&text_key("run.waiting")));
    }

    #[test]
    fn run_panel_shows_summary_and_current_subagent_detail() {
        let mut timeline = RunTimeline::new();
        timeline.apply_event_publish(&json!({
            "type": "model.allocation.selected",
            "payload": {
                "childId": "child-1",
                "providerId": "openai",
                "modelId": "gpt-5",
                "scope": "subagent-child"
            }
        }));
        timeline.apply_event_publish(&json!({
            "type": "subagent.child.start",
            "payload": { "childId": "child-1", "task": "inspect tool flow" }
        }));
        timeline.apply_event_publish(&json!({
            "type": "tool.started",
            "payload": {
                "childId": "child-1",
                "callId": "call-1",
                "server": "shell",
                "tool": "exec",
                "argsPreview": "rg model"
            }
        }));
        timeline.apply_event_publish(&json!({
            "type": "process.output",
            "payload": {
                "childId": "child-1",
                "callId": "call-1",
                "processId": "proc-1",
                "command": "rg model",
                "outputPreview": "model.allocation.selected"
            }
        }));
        timeline.apply_event_publish(&json!({
            "type": "executive.loop.paused",
            "payload": {
                "childId": "child-1",
                "askId": "ask-1",
                "reason": "needs decision",
                "crystalCandidate": "save learned route"
            }
        }));

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

        assert!(rendered.contains(&format!("{} 1", text_key("run.kind.model"))));
        assert!(rendered.contains("openai/gpt-5"));
        assert!(rendered.contains(&text_key("run.current")));
        assert!(rendered.contains("Shell rg model"));
        assert!(rendered.contains("子进程 rg model"));
        assert!(rendered.contains("ASK ask-1"));
        assert!(rendered.contains("结晶 save learned route"));
        assert!(!rendered.contains(&format!("{}\n", text_key("run.kind.subagents"))));
    }
}
