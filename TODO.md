# Flyflor CLI TODO

## 2026-05-27 ASK 公民权限与 Exo 闭环

- [x] 复盘 TUI ASK 提交路径，定位默认推荐项被当成普通输入发送的路径。
- [x] pending ASK 时普通输入保持普通文本；只有显式确认 ASK 菜单才发送结构化 ASK/permission answer。
- [x] 公民权限 ASK 展示为授权执行策略，区分普通 ASK 回答、权限授权、sandbox/tool audit event。
- [x] Exo timeline 禁止 `unknown`，最后一个 Exo 自动展开，其余折叠。
- [x] `execution.job.detail.get` 做去重/节流，避免重复拉取导致 socket 噪声。
- [x] 增加 Rust targeted tests，并运行 `cargo check`、`cargo test`。
- [x] 主控合并后复跑 `cargo fmt --check`、`cargo check`、`cargo test`，确认 TUI 闭环仍通过。

## 2026-05-27 真实 TUI 全链路场景

- [x] 新增 `smoke:live:tui`，通过 tmux 启动隔离 kernel socket 与真实 `flyflor-cli`，驱动 `/approve`、真实用户消息、`/ask`、`/history`、`/status`。
- [x] TUI live 场景保存完整 pane capture、CLI log、kernel log 和 report，并支持 `--keep-tmux` 保留 session 供人工查看。
- [x] 验收真实 TUI capture 无 `unknown`、无 panic、kernel 无 `turn.error`、可见 Flyflor/ASK surface。
- [x] 运行验证：`npm run smoke:live:tui -- --keep-tmux`、`cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`。

## 2026-05-27 最终真实 TUI 复核

- [x] 在安全 xtools 默认工具面收紧后复跑 `npm run smoke:live:tui -- --keep-tmux`，确认真实 TUI、kernel socket、ASK/history/status 路径仍 `ok: true`。
- [x] 复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`，确认 TUI ASK/Exo/detail 去重改动无回归。
