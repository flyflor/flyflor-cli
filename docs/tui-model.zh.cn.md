# TUI 模型

## 展示契约

TUI 会同时展示 kernel 状态和本地交互状态。kernel 状态是权威；本地交互状态用于让 UI 能平滑滚动、聚焦、复制、打开菜单和展示 pending request。

主屏幕包含：

- Header：显示 socket/history 状态、session label 和 turn count。
- 左侧 transcript：显示用户 prompt、assistant answer、context rows、ASK continuation rows、blackboard/replay rows、recall/thought rows 和 create fork 入口。
- 右侧面板：显示 TODO、model/status、context window 和 fork/memory。
- Composer footer：显示 interaction mode、active fork label 和本地命令提示。

## ASK 展示

ASK 数据来自 turn metadata 或 ASK snapshots。如果 metadata 包含 continuation（`snapshotId` 或 `continuationId`），transcript 会得到一个 `AskResume` context row。打开它会显示 ASK menu。

ASK menu 包含 kernel 提供的 options，并额外提供 `Other` 自由输入。选择固定 option 会发送带 continuation metadata 的 `gateway.message.send`。选择 `Other` 会把 continuation 暂存在本地，直到下一次 composer submit，再把用户输入文本和同一份 continuation metadata 一起发送。

CLI 不决定 ASK 语义。它只展示选项，并把用户选择的答案交回 kernel。

## Plan 展示

Plan 和 TODO 数据可以来自 task snapshots 或 turn metadata。右侧面板 TODO section 会推导 `PlanState`：

- Empty：没有 task rows，或只有 placeholder plan。
- Generating：状态文本表示正在生成。
- Awaiting confirmation：状态文本包含确认语义。
- Running：至少存在一个 active/current row。
- Abandoned：状态文本表示已放弃。

当 plan 等待确认时，`/todo` 会打开 plan menu。菜单提供确认、补充和放弃。确认与放弃会立即发送 `task.plan.decide`。补充会让 composer 进入 pending plan-revision input；下一次 submit 会发送带 `revision` 的 `task.plan.decide`。

CLI 不执行计划。它只展示 plan state，并发送用户的决策。

## Fork 展示

Fork 数据出现在三个位置：

- Transcript context rows 聚合 `planning.contextForks` metadata，并展示 fork 摘要。
- Create-fork row 或 `/fork` 命令会基于最新的结构化 assistant turn anchor 发送 `fork.create`。
- 当 CLI 进入 fork 时，header 和 composer footer 会展示 active fork session label。

当 `fork.snapshot` 创建 active fork session 后，CLI 会保存 root turns，清空 transcript 用于 fork 对话，请求 fork-scoped history，并在后续消息中携带 `context.contextForkId`。

`/exit fork` 会离开 fork session，并在可用时恢复 root 或 parent 状态。真实 fork 数据仍由 kernel 权威维护。

## Blackboard 展示

Blackboard 数据可以通过 metadata、snapshot 或 subscription event 到达：

- Turn metadata 可在左侧 transcript 创建 blackboard context rows。
- `blackboard.snapshot` 会变成一个 synthetic display turn。
- 如果 kernel 发出兼容的 blackboard runtime events，解析器仍能理解；但 CLI 目前不主动订阅临时 blackboard event type 名称。
- `/blackboard` 会把最新 blackboard summary 写入右侧面板 status line。

右侧下方面板目前不再把独立 blackboard stream 渲染成单独 section。测试保护当前右侧面板 section 集合：TODO、Model / Status、Context Window 和 Fork / Memory。

## 状态模型

可见状态模型由以下部分组合：

- `HistoryStatus`：loading、connected/live、mock/offline 或 error。
- `InteractionMode`：ACT、PLAN 或 YOLO。
- Pending work：pending turns、streaming footers 和 pending fork creation。
- 来自 `StatusSnapshot` 的 model/provider 字段。
- 来自 `StatusSnapshot` 的 context-window telemetry、本地 fallback 估算和可选环境变量 fallback。
- 通过右侧面板 status 暴露的 clipboard 与 socket error message。

状态 label 是展示提示，不应被用作持久状态或协议权威。

## 右侧面板

右侧面板使用 sticky layout：

- TODO 占据可伸缩的顶部区域，并拥有右侧滚动状态。
- Separator 将 TODO 和固定的下方 status 区域分隔开。
- 下方区域显示 Model / Status、Context Window 和 Fork / Memory。

左右方向键会在可复制的右侧面板 sections 之间移动焦点。按 `y` 时，如果存在 active selection，会复制 selection；否则复制当前聚焦的右侧面板 section。TODO list 有意与下方 scroll/copy model 分离。

## Hot Memory 与 Fork Memory

Context Window section 展示 hot context usage。它优先使用 kernel 提供的 context telemetry；当 kernel 没有提供时，从最近 turns 和 active fork id 做本地估算；当 kernel 没有提供最大窗口时，可以使用 `FLYFLOR_CONTEXT_WINDOW`。

Fork / Memory section 展示最多五条最近 fork 摘要，以及来自 `fork.memory` 数据的 `brain.db` label。`brain.db` label 只用于展示。`brain.db` 是 kernel 侧 ledger/query/replay/audit/detail 存储，不是 CLI prompt 容器。
