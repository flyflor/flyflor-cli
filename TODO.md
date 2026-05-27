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

## 2026-05-27 workmux CLI/Gateway 并发协调

- [x] 配置 `.workmux.yaml` 为独立 tmux session 的 Codex 并发 lane。
- [x] 创建 `session-table.md`，记录每个 lane 的 worktree、tmux attach、capture 和 send 命令。
- [x] 启动 docs-guardrails、main-rs-split、cli-shell、gateway-runtime、gateway-channels、npm-release 六个 Codex 子进程。

## 2026-05-27 Docs guardrails 同步

- [x] 同步 `AGENT.md`/`AGENTS.md`，明确 `docs-guardrails` lane 只处理 guardrails/docs 闭环，不实现 feature code。
- [x] 同步 README 与 docs 中 ASK、公民权限 metadata、Exo timeline、detail 去重和 CLI/gateway thin-client 口径。
- [x] 复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`。

## 2026-05-27 main.rs 低风险机械拆分

- [x] 将 `src/main.rs` 的 theme、input cursor/render/paste normalization、clipboard/OSC52 helper 机械拆到独立 owner module。
- [x] 复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`，确认拆分无行为回归。

## 2026-05-27 npm 顶层 CLI shell

- [x] 新增 Rust 顶层 CLI parser，默认 `flyflor` 保持进入现有 TUI。
- [x] `flyflor -h` 输出顶层 help，`flyflor gateway -h` 输出 gateway help，不进入 raw TUI。
- [x] 为 gateway-runtime 预留 command enum/types，不在 CLI 侧实现 channel adapters。
- [x] 主控合并后复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`。

## 2026-05-27 gateway runtime lane

- [x] 新增 CLI-owned gateway runtime module/API，暴露 foreground run、start、stop、restart、status、logs 与 runtime paths。
- [x] runtime pid/lock/status/log/stop files 落在 Flyflor CLI home，不使用 kernel `FLYFLOR_HOME`，不写 brain.db/scope.db/log DB。
- [x] foreground runtime 只通过 Flyflor `/ws` 连接 kernel，并复用 `flyflor.ws.v1` envelope bootstrap builders。
- [x] main 保留 daemon child env hook，并将 `flyflor gateway <run|start|stop|restart|status|logs>` 接到 runtime API。

## 2026-05-27 npm 全局安装包装

- [x] 增加 npm `bin` wrapper、package metadata、platform binary build/install scripts。
- [x] 增加 local `npm pack`/global-prefix install smoke，验证 wrapper、platform binary 落位和 CLI help。
- [x] 主控合并后复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`npm run smoke:npm:local`、`FLYFLOR_NPM_SMOKE_HELP=1 npm run smoke:npm:local`、`git diff --check`。

## 2026-05-27 gateway channels / Weixin iLink

- [x] 增加 gateway channel platform trait/registry，未来平台只返回 explicit unavailable，不假成功。
- [x] 增加 Weixin iLink adapter：账号/config 持久化、QR helper、getupdates long-poll、context_token store/echo、dedup TTL、retry/backoff/session-expired/rate-limit 分类、sendtyping/sendmessage payload 和 media unavailable metadata。
- [x] 增加 channel runtime bridge，将 normalized inbound message 通过现有 `gateway.message.send` / `/ws` 路径送入 Flyflor。
- [x] 主控合并后复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`。

## 2026-05-27 npm cross-target build 收口

- [x] `build:binary` 支持显式 Rust target triple，并输出到对应 `dist/<platform>-<arch>`。
- [x] `build:binary:all` 提供发布批量构建入口。
- [x] 复跑 host target 显式构建、npm local/global-prefix smoke、Cargo 门禁和 `git diff --check`。

## 2026-05-27 gateway bridge streaming

- [x] inbound normalized channel message 映射为 `gateway.message.send`，保留 routing/audit/reply anchor 与显式 `payload.context`。
- [x] ASK/approval metadata 作为结构化 metadata/context 透传，不把工具授权写成普通用户文本。
- [x] outbound 消费 `turn.delta`、`turn.final`、`turn.error` 与 subscribed `event.publish`，按 channel capability 选择 typing/send 或 edit/draft/card stream update。
- [x] channel capability report 显式标记 `available`/`degraded`/`unavailable`，未实现平台不返回假成功。
- [x] 增加 mock WS targeted tests 覆盖 inbound envelope、ASK/approval metadata、delta/final/error 和 event.publish streaming。
