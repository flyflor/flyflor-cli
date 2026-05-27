# Flyflor CLI 日志

## 2026-05-27

- 状态：进行中
  执行者：tui-ask-timeline
  范围：ask-citizen-permission-exo-timeline
  摘要：启动 TUI 侧 ASK 公民权限提交、Exo timeline 状态、history/fork/read-model 展示闭环修复。
  原因：pending ASK 默认选择曾被写成普通用户消息，且 Exo 工具/子进程状态出现 `unknown` 和黑盒卡住。
  验证：待运行 targeted tests、`cargo check`、`cargo test`。

- 状态：完成
  执行者：tui-ask-timeline
  范围：ask-citizen-permission-exo-timeline
  摘要：普通 composer 输入不再自动附带 pending ASK continuation；显式 ASK 菜单提交继续发送结构化 askAnswer，并为 continue-tools/keep-budget/keep-subagents 写入 citizenPermission metadata，同时普通消息文本不再写权限 token。Exo rows 默认仅最后一个展开，历史 rows 折叠；未知 subagent 状态展示为 pending；execution.job.detail.get 按 jobId 去重。
  原因：修复 ASK 推荐项/权限项被当成普通用户消息发送、Exo timeline 出现 unknown 与重复 detail 请求的问题。
  验证：`cargo fmt --check`、`cargo check`、`cargo test` 全部通过。

- 状态：完成
  执行者：main-codex
  范围：ask-citizen-permission-exo-timeline-merge-verification
  摘要：主控在内核 socket smoke 修复后复跑 TUI 格式、类型和单测验证，确认 ASK 菜单、公民权限 metadata、Exo timeline、detail 去重测试仍全绿。
  原因：TUI 是公民权限 ASK 的发送端，内核恢复契约修复后需要确认 TUI 结构化 payload 与展示层没有回退。
  验证：`cargo fmt --check`; `cargo check`; `cargo test`。

- 状态：完成
  执行者：main-codex
  范围：real-tui-loop-closure
  摘要：新增 `scripts/live-tui-scenario.ts` 与 `smoke:live:tui`，通过 tmux 启动隔离 `FLYFLOR_HOME` kernel socket 和真实 Rust TUI，自动驱动 `/approve`、真实用户消息、`/ask`、`/history`、`/status`，并保存 pane capture、CLI log、kernel log、report。
  原因：用户明确指出单元测试不够；TUI 必须在真实 socket、真实 provider、真实终端渲染路径中证明 pending ASK 普通输入不被劫持、Exo 不出现 `unknown`、失败能在日志和界面可见。
  验证：`npm run smoke:live:tui -- --keep-tmux` 输出 `ok: true`，报告目录 `/Users/yi./Desktop/yi/flyflors/flyflor-cli/.flyflor-cli/live/2026-05-27T05-01-28-378Z/`；`cargo fmt --check`; `cargo check`; `cargo test`; `git diff --check`。
  风险：`--keep-tmux` 会保留 `flyflor-live-kernel` 与 `flyflor-live-tui` session 供人工查看；不带该参数时脚本会清理隔离运行态。

- 状态：完成
  执行者：main-codex
  范围：real-tui-final-rerun
  摘要：在内核默认真实 xtools 工具面收紧后，重新运行真实 TUI tmux 场景，确认 TUI 仍能连接隔离 kernel socket、完成 `/approve`、真实消息、`/ask`、`/history`、`/status` 并产出完整 capture/log/report。
  原因：最终验收不能只依赖 earlier pass；内核工具面变化后要确认 TUI 可见性和终端交互没有退化。
  验证：`npm run smoke:live:tui -- --keep-tmux` 输出 `ok: true`，报告目录 `/Users/yi./Desktop/yi/flyflors/flyflor-cli/.flyflor-cli/live/2026-05-27T05-17-23-744Z/`；`cargo fmt --check`; `cargo check`; `cargo test`; `git diff --check`。
  风险：保留 `flyflor-live-kernel` 与 `flyflor-live-tui` tmux session 供人工查看完整交互。
