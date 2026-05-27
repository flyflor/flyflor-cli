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

- 状态：完成
  执行者：main-codex
  范围：workmux-cli-gateway-coordination
  摘要：配置 `.workmux.yaml` 为 session 模式 Codex 并发工作流，新增 `session-table.md` 记录 docs-guardrails、main-rs-split、cli-shell、gateway-runtime、gateway-channels、npm-release 六个 lane 的 worktree 路径、tmux attach、capture 和 send 命令，并启动对应 Codex 子进程。
  原因：CLI/gateway 壳和 TUI 解耦工作需要用户可直接监控每个子智能体的完整交互与 working 细节。
  验证：`workmux list`；`tmux list-sessions`；`tmux capture-pane -t <session>:0.0 -p -S -80`。
  风险：首次启动 Codex 出现 hook trust 提示，已选择不信任 hooks 继续运行；当前实际 tmux session 名因首次配置前缀叠加为 `wm-wm-*`，已在 `session-table.md` 中记录真实命令，并把后续配置改为不再自动叠加前缀。

- 状态：完成
  执行者：main-codex
  范围：workmux-shared-dependencies
  摘要：将 `.workmux.yaml` 增加 `files.symlink` 规则，让后续 worktree 共享 `node_modules` 与 `target`；同时为已创建的六个 worktree 手动建立 `node_modules -> ../../node_modules` 和 `target -> ../../target` 软链接，并把 `.worktrees/` 加入 `.gitignore`。
  原因：并发 Codex lane 需要共享依赖和构建缓存，避免每个 worktree 重装依赖或重复占用 Rust build 目录。
  验证：`ls -ld .worktrees/*/node_modules .worktrees/*/target`。
  风险：共享 `target` 能提升速度，但多个 lane 同时跑 `cargo` 时仍可能等待 Cargo build lock。

- 状态：完成
  执行者：docs-guardrails
  范围：cli-gateway-docs-redlines
  摘要：同步 `AGENT.md`/`AGENTS.md` docs lane 红线，并更新 README 与 docs 中 pending ASK 普通输入、公民权限 metadata、Exo timeline 状态、最后 Exo 展开、detail 请求去重和 CLI/gateway thin-client 口径。
  原因：文档仍有普通 typed ASK answer 自动复用 continuation metadata 的旧表述，需与当前 ASK/permission/Exo 闭环和 docs-guardrails lane 约束一致。
  验证：`cargo fmt --check`; `cargo check`; `cargo test`; `git diff --check`。

- 状态：进行中
  执行者：main-rs-split
  范围：main-rs-low-risk-split
  摘要：停止扩大拆分范围，仅保留低风险 owner module 提取：theme、input cursor/render/paste normalization、clipboard/OSC52 helper；不移动 socket/protocol/state code。
  原因：本 lane 收口当前安全拆分，避免在同一 pass 扩大到 socket payload 或 app state 行为面。
  验证：待运行 `cargo fmt --check`; `cargo check`; `cargo test`; `git diff --check`。

- 状态：完成
  执行者：cli-shell
  范围：npm-top-level-cli-shell
  摘要：新增 `src/cli_shell.rs` 作为 owned Rust 顶层 parser；默认无参数继续进入现有 Ratatui TUI；`-h/--help` 和 `gateway -h/--help` 在进入 raw mode 前输出帮助；gateway-runtime 仅定义预留 command enum，不在本侧实现 channel adapters。
  原因：npm 安装后的 `flyflor` 需要稳定 shell UX，同时不能破坏当前 TUI 或绕过既有 `/ws` kernel 边界。
  验证：主控待复跑 `cargo run -- -h`; `cargo run -- gateway -h`; `cargo fmt --check`; `cargo check`; `cargo test`; `git diff --check`。
  风险：`flyflor gateway run/status` 目前只解析为预留 enum，执行时明确提示 gateway-runtime adapters 不属于 flyflor-cli。

- 状态：完成
  执行者：gateway-runtime
  范围：cli-owned-gateway-shell-runtime
  摘要：新增 `src/gateway_runtime.rs`，提供 CLI-owned runtime API：foreground run、daemon start/stop/restart/status、log tail 和 runtime path report。runtime files 使用 `.flyflor-cli/gateway/{gateway.pid,gateway.lock,gateway.stop,status.json}` 与 `.flyflor-cli/logs/gateway.log`，daemon child 通过 `FLYFLOR_GATEWAY_RUNTIME_FOREGROUND=1` 进入 foreground hook。Flyflor bridge 只连接 `FLYFLOR_WS_URL`/默认 `/ws`，并复用 `GatewayClientBootstrap` 与 `EnvelopeFactory` 发送 `flyflor.ws.v1` bootstrap envelopes。
  原因：gateway runtime lane 只负责 CLI 侧 runtime/lifecycle 与 `/ws` bridge，不修改 kernel 或直接写 brain/scope/log DB。
  验证：主控待复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`。

- 状态：完成
  执行者：npm-release
  范围：npm-global-install-packaging
  摘要：为 `flyflor-cli` 增加 npm global install 包装：`flyflor` bin wrapper、platform binary `dist/<platform>-<arch>` build/install scripts、local npm pack/install smoke，并在 README 记录全局安装和 cross-build fallback。
  原因：发布前需要保证 `npm i -g flyflor-cli` 能安装 Rust TUI binary；cli-shell 负责 `flyflor -h` 与 `flyflor gateway -h` 的进程级 help 行为。
  验证：主控待复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`npm run smoke:npm:local`、`FLYFLOR_NPM_SMOKE_HELP=1 npm run smoke:npm:local`、`git diff --check`。

- 状态：完成
  执行者：gateway-channels
  范围：gateway-platform-weixin-ilink
  摘要：新增 `src/tui/gateway/channels/` platform/runtime/weixin modules；Weixin iLink adapter 覆盖账号/config 持久化、QR helper、long-poll getupdates、context_token store/echo、dedup TTL、错误分类、sendtyping/sendmessage payload 与 media unavailable metadata。未来 Telegram/Slack/Discord/Webhook/API/WeCom/WhatsApp 在 registry 中明确 unavailable，不返回假成功。
  原因：gateway shell 需要把 channel 交互搬到 CLI 侧，并通过 `/ws` 血管把 normalized inbound message 送入 Flyflor kernel，不能直接写 kernel DB。
  验证：主控待复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`。
  风险：channel runtime 当前作为可选 gateway surface 合入，主 TUI 尚未默认启动；模块声明暂时允许 dead_code，后续接入显式 CLI/env 开关时应移除。

- 状态：完成
  执行者：main-codex
  范围：workmux-cli-gateway-final-verification
  摘要：主控核对当前 worktree、workmux session table、AGENT/AGENTS guardrails、CLI shell、gateway runtime、Weixin iLink channel surface 和 npm 包装后，收口所有待验证 TODO 状态。
  原因：完成 CLI/gateway/TUI 壳合并后，需要用当前工作树证据证明 `flyflor -h`、`flyflor gateway -h`、Cargo 门禁和 npm 全局安装包装链路都可用。
  验证：`cargo fmt --check`; `cargo check`; `cargo test`（196 passed）；`cargo run -- -h`; `cargo run -- gateway -h`; `npm run smoke:npm:local`; `FLYFLOR_NPM_SMOKE_HELP=1 npm run smoke:npm:local`; `git diff --check`。
  风险：gateway channels 当前是 CLI/TUI 侧可选 surface，Weixin iLink 细节已建模并测试，真实生产账号和 kernel 事件 schema 仍需接入环境做 live 验收。

- 状态：完成
  执行者：main-codex
  范围：npm-cross-target-build
  摘要：`scripts/build-binary.cjs` 增加 Rust target triple 到 npm `dist/<platform>-<arch>` 的映射，支持 `--target`、`--target=<triple>`、`--all` 和 `FLYFLOR_NPM_RUST_TARGETS`；`package.json` 增加 `build:binary:all`，中英文 README 同步 npm 全局安装和交叉编译入口。
  原因：原 npm 包装只覆盖当前平台 build，不能作为发布前交叉编译入口；需要让 `npm i -g flyflor-cli` 的 bundled binary 目录可由发布流程明确产出。
  验证：`node scripts/build-binary.cjs --target "$(rustc -vV | sed -n 's/^host: //p')"`；`npm run smoke:npm:local`；`FLYFLOR_NPM_SMOKE_HELP=1 npm run smoke:npm:local`；`cargo fmt --check`；`cargo check`；`cargo test`（196 passed）；`git diff --check`。另以 unsupported target smoke 验证未知 triple 会失败退出。
  风险：非 host triple 是否能实际链接仍取决于本机安装的 Rust target 和系统 cross linker；脚本现在会把失败显式暴露给发布流程。

- 状态：完成
  执行者：dir-foundation
  范围：module-layout-only
  摘要：将 TUI helper modules 收拢到 `src/tui/`，将 context/layout 收拢到 `src/tui/context/` 与 `src/tui/layout/`，将 kernel WebSocket client/envelope/command/subscription 从 `src/tui/gateway/` 改名为 `src/tui/kernel/`；同时把 CLI parser 放入 `src/cli/`，把 gateway runtime 和 channel adapters 放入 `src/gateway/` 与 `src/gateway/channels/`。
  原因：为 CLI/gateway/TUI ownership 建立目录边界，保持 TUI 只通过 kernel socket/gateway payload 消费，不引入 session concept 或行为改动。
  验证：`cargo fmt --check`; `cargo check`; `cargo test`（196 passed）；`git diff --check`; `rg "tui::gateway|src/tui/gateway|crate::gateway" src` 仅剩新外部 `crate::gateway::channels` 路径。

- 状态：完成
  执行者：kernel-contract-audit
  范围：channel-identity-explicit-context-contract
  摘要：阅读 kernel socket/control/runtime turn docs 后，更新 CLI 协议与 TUI 文档，明确 channel identity 只映射到 `conversationKey`、`threadId`、`chatType`、`user` 和 gateway metadata；`payload.context` 只承载显式 `activeScope`、`contextForkId`、`skillNames`、`toolApprovals`。新增 gateway message payload tests，证明 channel identity 不会创建或污染 `payload.context`。
  原因：CLI/gateway 只能作为 thin client，经 `/ws` 发送 routing/audit metadata 与显式上下文，不能把 history/read-model snapshots 或 channel identity 当作 prompt context。
  验证：`cargo fmt --check`；`cargo check`；`cargo test`（198 passed）；`git diff --check`。
  风险：运行时代码中仍有历史内部函数/test 名称包含旧词汇，本次未做不相关重命名，避免扩大行为面。

- 状态：完成
  执行者：gateway-jsonc-config
  范围：gateway-jsonc-config-registry
  摘要：新增 CLI-owned `gateway.jsonc` schema、JSONC parser、init/validate/doctor/channel toggle helpers、Hermes-compatible channel registry、canonical aliases 与 env alias metadata；channel env fallback 在未设置 `FLYFLOR_GATEWAY_CHANNELS` 时读取默认 JSONC enabled channels。
  原因：gateway config 必须由 CLI 侧拥有，使用 JSONC 作为唯一配置格式，同时保持 no-session contract 和 explicit unavailable/degraded channel surface。
  验证：`cargo fmt --check`；`cargo check`；`cargo test`；`git diff --check`。
  风险：本 lane 只提供配置/schema/registry 能力，真实平台 listener 与 transport 由后续 channel lanes 接入。

- 状态：完成
  执行者：main-codex
  范围：src-tui-directory-alignment
  摘要：按最新目录约束，将已合并的 `cli`、`gateway runtime`、`gateway channels`、`gateway config`、`gateway platforms` 全部迁入 `src/tui`，并让 `src/main.rs` 只通过 `mod tui` 访问 `tui::cli` 与 `tui::gateway`。
  原因：用户已将此前功能迁入 `src/tui`，本轮只保证 `src/tui` 内部结构可用，避免继续维护外层 `src/cli` 或 `src/gateway` 分层。
  验证：`cargo fmt --check`；`cargo check`；`cargo test`（207 passed）；`git diff --check`。
  风险：`gateway config` 的部分 public API 当前为后续 CLI command wiring 预留，编译会提示 dead_code warning；本轮未扩大到未完成的全渠道 concrete adapter merge。

- 状态：进行中
  执行者：gateway-bridge-streaming
  范围：gateway-channel-ws-bridge-streaming
  摘要：扩展 channel bridge，将 normalized inbound message 构造成 `gateway.message.send`，保留 route anchor 与显式 `payload.context`；新增 channel capability report 和 generic stream update abstraction；outbound 侧消费 `turn.delta`、`turn.final`、`turn.error`、`event.publish`，按能力走 typing/send 或 edit/draft/card update fallback。
  原因：gateway bridge 必须作为 thin client 通过 `/ws` 与 Flyflor 交互，不能把 conversation/thread/user 当作 session，也不能把 ASK/approval 授权转成普通用户文本。
  验证：已通过 targeted `cargo test tui::gateway::channels::runtime::tests`；待复跑完整 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`。
  风险：Weixin iLink 当前明确为 send+typing 可用、edit/draft/card/media unavailable；未来平台只需在 adapter abstraction 中实现对应 stream update，不在 bridge runtime 写具体平台协议。

- 状态：完成
  执行者：gateway-bridge-streaming
  范围：gateway-channel-ws-bridge-streaming
  摘要：完成 channel bridge streaming 收口：inbound `NormalizedInboundMessage` 支持显式 context 并构造成 `gateway.message.send`；metadata 注入 explicit capability report；outbound 按 capability 消费 delta/final/error/event.publish，send-only channel 使用 typing/final send，stream channel 使用 edit/draft/card update abstraction 并在 final 失败时 fallback send。
  原因：满足 gateway bridge lane 对 `/ws` thin-client、ASK/approval structured metadata/context 和 channel capability degradation 的契约。
  验证：`cargo test tui::gateway::channels::runtime::tests`；`cargo fmt --check`；`cargo check`；`cargo test`（199 passed）；`git diff --check`。
  风险：真实平台 live stream update 仍取决于未来 adapter 是否实现 `stream_update`；当前 Weixin iLink 明确报告 edit/draft/card/media unavailable。
