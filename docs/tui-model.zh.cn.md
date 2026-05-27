# TUI 模型

## Display Contract

TUI 同时显示 kernel state 和 local interaction state。Kernel state 是权威；local interaction state 只用于滚动、聚焦、复制、打开菜单和平滑展示 pending requests。

主屏幕包含：

- Header：socket/history status、connection/fork label 和 turn count。
- 左侧 transcript：显示 user prompts、assistant answers、context rows、ASK continuation rows、blackboard/replay rows、recall/thought rows 和 create fork affordances。
- 右侧 panel：TODO、Run timeline、model/status、context window 和 fork/memory。
- Composer footer：interaction mode、active fork label 和本地 command hints。

Composer 使用 `Enter` 发送，使用 `Shift+Enter` 换行。

## ASK Display

ASK 数据来自 turn metadata 或 ASK snapshots。如果 metadata 包含 continuation（`snapshotId` 或 `continuationId`），transcript 会得到一个 `AskResume` context row。打开它会显示 ASK menu。

ASK menu 包含 kernel-provided options，并额外提供 `Other` 自由输入。选择固定 option 会发送带 continuation metadata 的 `gateway.message.send`。选择 `Other` 会启动一次显式 continuation reply；下一次 composer submit 会把用户输入文本和同一份 continuation metadata 一起发送。

pending ASK 不会把普通 composer 变成默认 ASK answer。如果用户没有显式选择 ASK menu option 或 `Other` 就直接输入，CLI 会发送不带 continuation metadata 的普通消息文本。

CLI 不决定 ASK 语义。它只展示选项，并把用户选择的答案交回 kernel。

## Plan Display

Plan 和 TODO 数据可以来自 task snapshots 或 turn metadata。右侧面板 TODO section 会推导 `PlanState`：

- Empty：没有 task rows，或只有 placeholder plan。
- Generating：status text 表示正在生成。
- Awaiting confirmation：status text 包含确认语义。
- Running：至少有一条 active/current row。
- Abandoned：status text 表示已放弃。

当 plan 等待确认时，`/todo` 会打开 plan menu。菜单提供 confirm、revise 和 abandon。Confirm 与 abandon 会立即发送 `task.plan.decide`。Revise 会让 composer 进入 pending plan-revision input；下一次 submit 发送带 `revision` 的 `task.plan.decide`。

CLI 不执行 plan。它只展示 plan state 并发送用户决策。

## Fork Display

Fork data 出现在三处：

- Transcript context rows 聚合 `planning.contextForks` metadata，并展示 fork summaries。
- Create-fork row 或 `/fork` command 基于最新 structured assistant turn anchor 发送 `fork.create`。
- 当 CLI 进入 fork 时，header 和 composer footer 显示 active fork label。

当 `fork.snapshot` 进入 active fork view 后，CLI 保存 root turns，清空 transcript 用于 fork conversation，请求 fork-scoped history，并在后续消息中携带 `context.contextForkId`。

`/exit fork` 离开 active fork view，并在可用时恢复 root 或 parent display state。真实 fork data 仍由 kernel 权威维护。

## Blackboard Display

Blackboard data 可以通过 metadata、snapshots 或 subscription events 到达：

- Turn metadata 可以在左侧 transcript 创建 blackboard context rows。
- `blackboard.snapshot` 变成 synthetic display turn。
- 固定 gateway subscription 包含稳定 `blackboard.*` runtime events。这些 events 显示在 Run 中，让过程可见，而不是藏在 snapshots 后。
- `/blackboard` 把 latest blackboard summary 复制到右侧 panel status line。

CLI 不调度 workers，不决定 route convergence，也不写 Blackboard state。

## Run Timeline

Run 是 gateway events 的可见执行脊柱。它消费 `event.publish`、`event.snapshot`、`event` 和 `execution.job.snapshot` data，并展示 route decisions、scope recall、blackboard turns、tool calls、ASK pauses、plan writes、forks、Executive loop transitions、process events、worker events 和 subagent lifecycle updates。

Job detail fetches 按 job id 去重或节流。Timeline display 不应在渲染期间反复请求同一个 `execution.job.detail.get` payload。

Subagent events 会合并成 batch/child tree。Loose child events 会在后续 snapshot 或 batch event 到来时挂到对应 batch，重复 snapshot 更新已有 rows 而不是重复追加。这样 subagents 可见，但 CLI 不负责 runtime scheduling。

## Tool Visibility And Approval Closure

Tool events 和 execution-job snapshots 让 Executive 外骨骼可见。CLI 可以展示 tool start/progress/success/failure、MCP tool execution、loop pause/resume 和 budget exhaustion。

kernel `toolApprovals.mcpToolCalls` 和 `toolApprovals.userToolCalls` 的普通 per-turn approval 已通过 `/approve` 闭环。它只标记下一次发送，发送后清除。YOLO mode 仍是单独的本地高权限 interaction metadata。

`continue-tools`、`keep-budget`、`keep-subagents` 等公民权限 options 会展示为授权策略 choices，并作为结构化 metadata 发送。它们不得被转换成普通用户消息文本。

当 turn 处于 active 状态时，footer 会显示动画 Working 行。按一次 Esc 会进入终断预备状态；在窗口期内再按一次 Esc，会按 pending public message id 发送 `gateway.message.interrupt`。

Exo tool/subprocess 区域使用解析后的 tool name 和 lifecycle summary，不展示 `unknown` placeholder。等待权限、运行、完成、失败状态都必须有明确 label。最后一个 Exo row 自动展开；历史 Exo rows 默认折叠。展开行显示最新输出片段，但 CLI 仍不本地执行工具。

`/undo` 打开用户消息 rollback anchor 菜单。确认一行会发送 `gateway.message.undo`；kernel 侧 memory abandonment 和 ledger audit 才是权威。

## Status Model

可见 status model 组合：

- `HistoryStatus`：loading、connected/live、mock/offline 或 error。
- `InteractionMode`：ACT、PLAN 或 YOLO。
- Pending work：pending turns、streaming footers 和 pending fork creation。
- 来自 `StatusSnapshot` 的 model/provider fields。
- 来自 `StatusSnapshot` 的 context-window telemetry、本地 fallback estimation 和可选 environment fallback。
- 右侧 panel status 中展示的 clipboard 与 socket error messages。

Status labels 是展示提示，不能当作 durable state 或 protocol authority。

## Right Panel

右侧 panel 使用 sticky layout：

- TODO 占据 flexible top area，并拥有 right scroll state。
- Separator 把 TODO 与 fixed lower status area 分开。
- Lower area 显示 Run、Model / Status、Context Window 和 Fork / Memory。

左右箭头在可复制 right-panel sections 间移动焦点。按 `y` 时，如果有 active selection 则复制 selection，否则复制 focused right-panel section。TODO list 有意独立于 lower scroll/copy model。

## Hot Memory and Fork Memory

Context Window section 展示 hot context usage。它优先使用 kernel-provided context telemetry；当 kernel 没有提供时，从最近 turns 和 active fork id 做本地估算；当 kernel 没有提供最大窗口时，可以使用 `FLYFLOR_CONTEXT_WINDOW`。Kernel 提供的 `contextWindowTokens` 是权威值，可以表示 1M-token model 这类大窗口 provider。

Fork / Memory section 展示最多五条最近 fork summaries，以及来自 `fork.memory` data 的 `brain.db` label。`brain.db` label 只用于展示。`brain.db` 是 kernel 侧 ledger/query/replay/audit/detail storage，不是 CLI prompt container。
