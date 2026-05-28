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

- 状态：完成
  执行者：main-codex
  范围：src-tui-external-tool-failure-visibility
  摘要：在 `src/tui/run_timeline/parser.rs` 增加 nested failure/unavailable detail 提取，并让 `src/tui/subagent/parser.rs` 复用该提取；外部 browser/computer sidecar、delegate、provider 返回的 `result.response.error/reason/code` 会显示为紧凑错误文本，不再回退成 raw JSON 或空白。
  原因：真实工具闭环中失败/不可用必须通过 TUI 可见，避免 Exo timeline 和 subagent 展开区进入黑盒状态。
  验证：待运行 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`。
  风险：仅扩展显示层 payload 提取，不改变 ASK、gateway socket、kernel ledger 或工具调度语义。

- 状态：完成
  执行者：main-codex
  范围：src-tui-external-tool-failure-visibility-verification
  摘要：完成外部工具失败可见性修复的格式、类型、全量测试和 diff whitespace 验证。
  原因：上一条记录在实现落地时预留验证状态；本条按 append-only 规则补充实际验证结果。
  验证：`cargo fmt --check`；`cargo check`（仅既有 gateway config dead_code warnings）；`cargo test`（209 passed）；`git diff --check`。
  风险：未运行真实 tmux TUI smoke；本次改动只覆盖 `src/tui` parser/display payload 消费，真实 LLM kernel 工具闭环已由上游 kernel smoke 覆盖。

- 状态：完成
  执行者：main-codex
  范围：src-tui-real-smoke-after-failure-visibility
  摘要：补跑真实 tmux TUI smoke，隔离启动 kernel socket 与 Rust TUI，确认本次 parser/display 改动不破坏真实 TUI loop。
  原因：用户要求真实场景不能只依赖单元测试；即使本次是显示层小改，也需要保留 live 证据。
  验证：`npm run smoke:live:tui` 输出 `ok: true`、`failedChecks: []`，报告目录 `.flyflor-cli/live/2026-05-27T13-08-49-999Z/`。
  风险：该 live smoke 验证 TUI 真实交互闭环与无 `unknown`/panic；nested external failure 形态由 targeted parser tests 覆盖。

## 2026-05-28

- 状态：进行中
  执行者：main-codex
  范围：channel-registry-codex-lanes
  变动文件：`src/tui/gateway/platforms.rs`、`src/tui/gateway/config.rs`、`src/main.rs`、`scripts/codex-lanes.sh`、`package.json`、`AGENTS.md`、`TODO.md`、`LOGS.md`
  摘要：将 gateway channel registry 扩展为全量 channel surface，使用中性 `source_channel`、`native_runtime`、capability feature、required/optional env 和 details 元数据；新增固定 worktree/tmux/Codex lane 脚本，输出 attach/capture 命令并软链依赖缓存。
  原因：用户要求支持参考项目的所有 channel 和细节，同时要求源码不出现参考项目关键字，并且必须能查看子 Codex working 细节。
  验证：已运行 `cargo check` 通过；待运行 `cargo fmt --check`、`cargo test`、`git diff --check`。
  风险：本条先闭合 registry/config/doctor/协作脚手架，除 Weixin 现有 adapter 外，其余平台仍标记 `planned` 并 explicit unavailable，真实 adapter 需要后续按 lane 分批落地。

- 状态：完成
  执行者：main-codex
  范围：gateway-channel-doctor-core
  变动文件：`src/cli/mod.rs`、`src/main.rs`、`src/tui/gateway/config.rs`、`TODO.md`、`LOGS.md`
  摘要：新增 `flyflor gateway channel doctor <name>`，并在 doctor item 中加入 availability；`config doctor` 同步输出 availability。新增 generic core tests，证明全 27 channel 在 required env 缺失时均为 explicit unavailable，planned channel 即使 env 存在也不会返回假 available。
  原因：真实 adapter 分 lane 落地前，必须先把 channel status/doctor 的失败态和不可假成功契约固定住，避免 TUI/gateway 把未实现通道当成可用执行面。
  验证：`cargo fmt --check`；`cargo check`；`cargo test`（216 passed）。
  风险：本轮只覆盖 doctor/status 契约；除既有 Weixin native runtime 外，planned channel 的真实 inbound/outbound adapter 仍未落地。

- 状态：完成
  执行者：main-codex
  范围：live-tui-send-closure
  变动文件：`src/main.rs`、`scripts/live-tui-scenario.ts`、`TODO.md`、`LOGS.md`
  摘要：修复 tmux 下普通 Enter 被当成换行导致 live smoke 只把内容留在 composer 的问题；live TUI 脚本改为等待 CLI socket connected 后再驱动，并要求 kernel log 出现真实 `gateway.message.send` 与 `mcp.tool.call.executed`。
  原因：用户要求 Rust TUI 必须用真实交互测试，不接受只渲染界面或 mock；原 smoke 断言过弱，无法证明 TUI 到内核工具调用闭环。
  验证：`cargo fmt --check && cargo test --quiet`（217 passed）；`npm run smoke:live:tui`（ok true，failedChecks 空，报告目录 `.flyflor-cli/live/2026-05-27T17-24-11-303Z/`）。
  风险：本轮只修复提交键和 smoke 强度；Confirm/ASK 视觉与协议分层仍待后续拆分。

- 状态：完成
  执行者：main-codex
  范围：cli-final-verification
  变动文件：`LOGS.md`
  摘要：补充最终验证证据：真实 TUI smoke 复跑到最新报告目录，本地 npm pack/install smoke 通过，diff whitespace 检查通过。
  原因：提交前需要证明 Rust TUI、npm 全局安装路径和工作区差异均处于可交付状态。
  验证：`npm run smoke:live:tui`（ok true，报告目录 `.flyflor-cli/live/2026-05-27T17-34-31-376Z/`）；`npm run smoke:npm:local`（local npm pack/install smoke passed）；`git diff --check`。
  风险：`Confirm` 独立 UI 仍未拆出；当前 TUI 继续消费既有 sandbox approval/ASK metadata。

- 状态：进行中
  执行者：main-codex
  范围：confirm-ask-display-split
  变动文件：`AGENTS.md`、`src/main.rs`、`docs/tui-model.md`、`docs/tui-model.zh.cn.md`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：公民权限/高风险授权菜单改为 `Confirm 授权执行策略` 标题，普通 ASK 仍保留 `ASK` 标题；文档同步声明 Confirm 不使用 ASK 结晶样式。
  原因：用户要求 ASK 是一级公民并可能结晶，Confirm 只是确认交互，CLI/TUI 必须分开渲染。
  验证：待运行 `cargo fmt --check`、`cargo check --all-targets`、`cargo test`、`git diff --check`。
  风险：本轮仅调整 TUI 显示与测试断言，不改现有视觉布局、不让 CLI 写 kernel DB，也不把 Confirm 语义下沉到内核。

- 状态：完成
  执行者：main-codex
  范围：confirm-ask-display-split-verification
  变动文件：同上
  摘要：完成 Confirm/ASK 显示拆分的格式、类型、全量测试和 whitespace 验证。
  原因：确认公民权限授权不再以 ASK 标题展示，同时普通 ASK 菜单保持原路径。
  验证：`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（218 passed）；`git diff --check`。
  风险：后续若内核新增独立 Confirm snapshot/event，还需要把当前 ASK-compatible permission metadata 迁到专门 Confirm 组件。

- 状态：进行中
  执行者：main-codex
  范围：confirm-answer-metadata-client
  变动文件：`src/tui/ask/command.rs`、`src/main.rs`、`src/tui/gateway/channels/runtime.rs`、`docs/protocol.md`、`docs/protocol.zh.cn.md`、`docs/tui-model.md`、`docs/tui-model.zh.cn.md`、`TODO.md`、`LOGS.md`
  摘要：公民权限/高风险授权发送 `metadata.confirmAnswer`，同时保留兼容 `metadata.askAnswer`；普通 ASK continuation 仍只走 ASK metadata。TUI 发送行 footer 区分 confirm answer 与 ask answer。
  原因：内核已将 Confirm 结构化入口提升为 `confirmAnswer`，CLI 需要同步，不再把确认授权作为新协议主入口的 ASK answer。
  验证：已运行 focused `cargo fmt --check`、`cargo test tui::ask::command::tests`、`cargo test ask_citizen_permission_menu_sends_metadata_without_token_message_text`、`cargo test mock_ws_inbound_send_envelope_preserves_route_context_and_ask_metadata`；待运行 `cargo check --all-targets`、`cargo test`、`git diff --check`。
  风险：本轮仍保留 `askAnswer` 兼容字段，后续内核提供独立 Confirm snapshot/event 后可移除 ASK-compatible fallback。

- 状态：完成
  执行者：main-codex
  范围：confirm-answer-metadata-client-verification
  变动文件：同上
  摘要：完成 CLI Confirm metadata 发送切片的 focused、格式、类型、全量测试和 whitespace 验证。
  原因：确认 TUI 公民权限路径发送 `confirmAnswer`，普通 ASK 路径仍保持 `askAnswer`，gateway channel bridge 透传 confirm metadata。
  验证：`cargo fmt --check`；`cargo test tui::ask::command::tests`（5 passed）；`cargo test ask_citizen_permission_menu_sends_metadata_without_token_message_text`（1 passed）；`cargo test mock_ws_inbound_send_envelope_preserves_route_context_and_ask_metadata`（1 passed）；`cargo check --all-targets`；`cargo test`（218 passed）；`git diff --check`。
  风险：仍保留 `askAnswer` 兼容字段，后续可随独立 Confirm read-model/event 删除。

- 状态：进行中
  执行者：main-codex
  范围：confirm-event-timeline-client
  变动文件：`src/kernel/subscription.rs`、`src/tui/run_timeline/state.rs`、`src/tui/run_timeline/view.rs`、`src/tui/run_timeline/parser.rs`、`src/tui/subagent/parser.rs`、`docs/protocol.md`、`docs/protocol.zh.cn.md`、`docs/tui-model.md`、`docs/tui-model.zh.cn.md`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：CLI 订阅 `confirm.answered`，Run timeline 以 Confirm row 展示，Subagent tree 用它关闭 pending needs-user marker。
  原因：内核已经提供 Confirm 独立事件后，thin client 需要显示 Confirm 生命周期，而不是继续只靠 ASK-compatible payload。
  验证：待运行 `cargo fmt --check`、focused tests、`cargo check --all-targets`、`cargo test`、`git diff --check`。
  风险：本轮只接入事件展示；完整独立 Confirm component/read-model 仍待后续实现，ASK-compatible fallback 暂时保留。

- 状态：完成
  执行者：main-codex
  范围：confirm-event-timeline-client-verification
  变动文件：同上
  摘要：完成 CLI Confirm event timeline 切片的格式、focused、类型、全量测试和 whitespace 验证。
  原因：确认 `confirm.answered` 已进入固定 subscription、Run timeline 和 subagent pending-user 闭合路径。
  验证：`cargo fmt --check`；`cargo test subscription_list_is_fixed_to_known_runtime_events`（1 passed）；`cargo test parses_required_event_families`（1 passed）；`cargo test ask_pause_and_answer_preserve_crystal_closure`（1 passed）；`cargo check --all-targets`；`cargo test`（218 passed）；`git diff --check`。
  风险：完整独立 Confirm component/read-model 仍待后续实现，ASK-compatible fallback 暂时保留。

- 状态：进行中
  执行者：main-codex
  范围：confirm-read-model-client
  变动文件：`src/kernel/command.rs`、`src/kernel/client.rs`、`src/kernel/subscription.rs`、`src/main.rs`、`docs/protocol.md`、`docs/protocol.zh.cn.md`、`docs/tui-model.md`、`docs/tui-model.zh.cn.md`、`TODO.md`、`LOGS.md`
  摘要：CLI bootstrap 新增 `confirm.list`，解析 `confirm.snapshot` 并恢复为 Run timeline 的 Confirm row，不生成 ASK continuation row。
  原因：内核已提供 Confirm read-model queries，CLI 需要在重连/启动时恢复最近 confirmation-only audit visibility，同时保持 ASK 与 Confirm 分层。
  验证：已运行 `cargo fmt --check`、`cargo test bootstrap_preserves_command_order`、`cargo test bootstrap_order_is_wire_contract`、`cargo test confirm_snapshot_restores_confirm_timeline_row`、`cargo test gateway_message_builder_can_confirm_tools_without_yolo`；待运行 `cargo check --all-targets`、`cargo test`、`git diff --check`。
  风险：本轮只做 read-model 恢复显示；完整独立 Confirm component UI 与移除 ASK-compatible fallback 仍留后续。

- 状态：完成
  执行者：main-codex
  范围：confirm-read-model-client-verification
  变动文件：同上
  摘要：完成 CLI Confirm read-model bootstrap/query 消费切片的 focused、格式、类型、全量测试和 whitespace 验证。
  原因：确认 `confirm.list` 启动查询和 `confirm.snapshot` Run timeline 恢复不破坏 ASK/Confirm 分层，也不改 TUI 视觉结构。
  验证：`cargo fmt --check`；`cargo test bootstrap_preserves_command_order`；`cargo test bootstrap_order_is_wire_contract`；`cargo test confirm_snapshot_restores_confirm_timeline_row`；`cargo test gateway_message_builder_can_confirm_tools_without_yolo`；`cargo check --all-targets`；`cargo test`（219 passed）；`git diff --check`。
  风险：完整独立 Confirm component UI 与移除 ASK-compatible fallback 仍留后续。

- 状态：进行中
  执行者：main-codex
  范围：confirm-component-foundation-client
  变动文件：`src/tui/confirm/mod.rs`、`src/tui/confirm/parser.rs`、`src/tui/confirm/state.rs`、`src/tui/mod.rs`、`src/main.rs`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 Confirm read-model owner，`confirm.snapshot` 先恢复到 `ConfirmState`，再投影为 Run timeline Confirm row。
  原因：继续拆开 ASK 与 Confirm；Confirm 是授权/确认 audit，不应继续由 `main.rs` 内联伪造成 ASK-compatible runtime event。
  验证：已运行 `cargo fmt --check`、`cargo test confirm_snapshot_restores_confirm_timeline_row`、`cargo test snapshot_records_keep_confirm_separate_from_ask`、`cargo test parses_required_event_families`；待运行 `cargo check --all-targets`、`cargo test`、`git diff --check`。
  风险：本轮只新增 state owner 和 read-model 投影，不改 TUI 视觉；发送路径仍保留 `askAnswer` 兼容字段，后续再移除 fallback。

- 状态：完成
  执行者：main-codex
  范围：confirm-component-foundation-client-verification
  变动文件：同上
  摘要：完成 Confirm read-model owner 切片的 focused、格式、类型、全量测试和 whitespace 验证。
  原因：确认 `ConfirmState` 恢复路径、Run timeline 投影和 ASK continuation 隔离无回归。
  验证：`cargo fmt --check`；`cargo test confirm_snapshot_restores_confirm_timeline_row`；`cargo test snapshot_records_keep_confirm_separate_from_ask`；`cargo test parses_required_event_families`；`cargo check --all-targets`；`cargo test`（220 passed）；`git diff --check`。
  风险：发送路径仍保留 `askAnswer` 兼容字段，后续需在完整迁移后移除 fallback。

- 状态：进行中
  执行者：main-codex
  范围：confirm-send-without-ask-fallback
  变动文件：`src/tui/ask/command.rs`、`src/main.rs`、`docs/protocol.md`、`docs/protocol.zh.cn.md`、`docs/tui-model.md`、`docs/tui-model.zh.cn.md`、`TODO.md`、`LOGS.md`
  摘要：公民权限/高风险授权发送路径移除 ASK-compatible `metadata.askAnswer` fallback，只发送 `metadata.confirmAnswer`、`citizenPermission` 和 continuation；普通 ASK continuation 仍走 `askAnswer`。
  原因：内核已提供 Confirm metadata、event、read-model，CLI 已有 Confirm owner，继续发送 ASK-compatible fallback 会模糊 Confirm/ASK 分层。
  验证：待运行 focused tests、格式、类型、全量测试和 whitespace 验证。
  风险：旧客户端兼容仍由内核保留；本轮只改变 CLI 新发送路径，不改 TUI 视觉。

- 状态：完成
  执行者：main-codex
  范围：confirm-send-without-ask-fallback-verification
  变动文件：同上
  摘要：完成 Confirm 发送路径移除 ASK fallback 的 focused、格式、类型、全量测试和 whitespace 验证。
  原因：确认公民权限授权只携带 `confirmAnswer`，普通 ASK 仍保留 `askAnswer`，TUI 视觉和 gateway 透传不回退。
  验证：`cargo fmt --check`；`cargo test tui::ask::command::tests`（5 passed）；`cargo test ask_citizen_permission_menu_sends_metadata_without_token_message_text`；`cargo test mock_ws_inbound_send_envelope_preserves_route_context_and_ask_metadata`；`cargo check --all-targets`；`cargo test`（220 passed）；`git diff --check`。
  风险：旧客户端兼容仍由内核保留；CLI 新发送路径已不再使用 ASK-compatible permission fallback。

- 状态：进行中
  执行者：main-codex
  范围：telegram-native-channel-adapter
  变动文件：`src/tui/gateway/channels/telegram.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/platforms.rs`、`src/tui/gateway/config.rs`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 Telegram Bot API native adapter，接入 gateway channel registry 与 doctor availability；保留 CLI 只通过 `/ws` 接通内核，不写 kernel DB。
  原因：推进 `gateway-channels-*` lane，从 western channels 的 Telegram 开始落地真实 adapter，而不是只返回 planned/unavailable。
  验证：已运行 `cargo test telegram -- --nocapture`（4 passed）与 `cargo test gateway -- --nocapture`（33 passed）；待运行格式、类型、全量测试和 whitespace 验证。
  风险：本轮不启用 Telegram streaming edit 的 runtime 占位消息锚；后续需要补 bot message id route anchor 和真实 sandbox smoke。

- 状态：完成
  执行者：main-codex
  范围：telegram-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 Telegram native adapter 第一阶段的 focused、gateway、格式、类型、全量测试和 whitespace 验证。
  原因：确认 Telegram registry/native doctor、inbound normalization、outbound send/typing、explicit media unavailable 和 planned-channel 不假成功契约无回归。
  验证：`cargo test telegram -- --nocapture`（4 passed）；`cargo test gateway -- --nocapture`（33 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（225 passed）；`git diff --check`。
  风险：后续仍需真实 Bot API sandbox smoke，以及 runtime 记录 bot message id 后再打开 stream edit 能力。

- 状态：完成
  执行者：main-codex
  范围：telegram-native-channel-adapter-capability-correction
  变动文件：`src/tui/gateway/channels/telegram.rs`、`TODO.md`、`LOGS.md`
  摘要：Telegram adapter 保留 `editMessageText` 实现骨架，但 capability report 暂不声明 edit 可用。
  原因：现有通用 runtime 在 delta 阶段只有 inbound message id，没有先发 bot message id anchor；过早声明 edit 会让 streaming path 假成功或持续失败。
  验证：`cargo fmt --check && cargo test telegram -- --nocapture && cargo test gateway -- --nocapture && cargo check --all-targets && cargo test && git diff --check`（通过，targeted 5 passed、gateway 33 passed、全量 225 passed）。
  风险：后续打开 Telegram stream edit 前必须先补 route anchor/placeholder message smoke。

- 状态：进行中
  执行者：main-codex
  范围：gateway-edit-stream-route-anchor
  变动文件：`src/tui/gateway/channels/runtime.rs`、`src/tui/gateway/channels/telegram.rs`、`TODO.md`、`LOGS.md`
  摘要：Gateway runtime 为 edit-capable channel 增加占位消息发送与 bot message id 保存，后续 delta/final 复用该 id 调 `stream_update`。
  原因：Telegram 这类平台需要先发一条 bot 消息才能编辑，不能把 inbound message id 当作 outbound edit target。
  验证：已运行 `cargo fmt --check`、`cargo test mock_ws_edit_stream_sends_placeholder_then_edits_channel_message -- --nocapture`、`cargo test telegram -- --nocapture`、`cargo test gateway -- --nocapture`；待运行类型、全量和 whitespace 验证。
  风险：真实 Telegram sandbox smoke 仍待凭据环境验证。

- 状态：完成
  执行者：main-codex
  范围：gateway-edit-stream-route-anchor-verification
  变动文件：同上
  摘要：完成 edit-capable channel stream route anchor 的 focused、gateway、类型、全量测试和 whitespace 验证。
  原因：确认 delta 首次发送占位消息、保存 bot message id、后续 delta/final 编辑同一 channel message，Telegram 可安全声明 edit streaming。
  验证：`cargo fmt --check`；`cargo test mock_ws_edit_stream_sends_placeholder_then_edits_channel_message -- --nocapture`（1 passed）；`cargo test telegram -- --nocapture`（5 passed）；`cargo test gateway -- --nocapture`（34 passed）；`cargo check --all-targets`；`cargo test`（226 passed）；`git diff --check`。
  风险：真实 Telegram sandbox smoke 仍待凭据环境验证。

- 状态：进行中
  执行者：main-codex
  范围：webhook-native-channel-adapter
  变动文件：`src/tui/gateway/channels/webhook.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 Webhook native adapter，本地 HTTP POST 入站归一化到 gateway bridge，outbound 通过 `WEBHOOK_PUBLIC_URL` callback 发送结构化 reply payload。
  原因：继续推进 western/longtail channel adapter 真实闭环，选择可本地验证且不需要第三方账号的 Webhook 作为第二个新增 native channel。
  验证：已运行 `cargo fmt --check`、`cargo test webhook -- --nocapture`（5 passed）、`cargo test native_runtime_status_only_marks_implemented_adapters -- --nocapture`、`cargo test gateway -- --nocapture`（40 passed）；待运行类型、全量和 whitespace 验证。
  风险：本轮不启动真实 listener smoke；后续需覆盖 HTTP POST -> `/ws` -> callback 的 live 场景。

- 状态：完成
  执行者：main-codex
  范围：webhook-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 Webhook native adapter 第一阶段的 focused、gateway、类型、全量测试和 whitespace 验证。
  原因：确认 Webhook secret/source 校验、context/metadata 入站归一化、outbound callback unavailable/degraded 语义、doctor availability 和 native runtime 状态红线无回归。
  验证：`cargo fmt --check`；`cargo test webhook -- --nocapture`（5 passed）；`cargo test native_runtime_status_only_marks_implemented_adapters -- --nocapture`（1 passed）；`cargo test gateway -- --nocapture`（40 passed）；`cargo check --all-targets`；`cargo test`（232 passed）；`git diff --check`。
  风险：真实 listener 到 kernel `/ws` 再到 callback 的 live smoke 仍待后续补齐。

- 状态：完成
  执行者：main-codex
  范围：webhook-live-smoke-closure
  变动文件：`src/tui/gateway/runtime.rs`、`scripts/webhook-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：`flyflor gateway run` 现在会启动 channel runtime；新增 `smoke:gateway:webhook`，用 mock `/ws` kernel 和 callback server 验证 Webhook 真实进程闭环。
  原因：Webhook adapter 单测不足以证明 channel runtime 被 daemon 启动，也不足以证明 HTTP 入站、`gateway.message.send`、`turn.final` 和 callback delivery 连通。
  验证：`npm run smoke:gateway:webhook`（ok: true）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（232 passed）；`git diff --check`。
  风险：本 smoke 使用 mock kernel，不依赖真实 Flyflor 内核推理；后续完整跨仓库 live 可复用同一 webhook path。

- 状态：进行中
  执行者：main-codex
  范围：ntfy-native-channel-adapter
  变动文件：`src/tui/gateway/channels/ntfy.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 ntfy native adapter，支持 JSON/JSONL poll 入站、HTTP POST publish 出站、sender allowlist、topic route 和 metadata normalization。
  原因：继续推进 longtail channel adapter 真实落地，扩大 native channel 覆盖，同时保持 planned channel explicit unavailable 红线。
  验证：已运行 `cargo fmt --check`、`cargo test ntfy -- --nocapture`（5 passed）、`cargo test gateway -- --nocapture`（45 passed）；待运行类型、全量和 whitespace 验证。
  风险：本轮不启动 ntfy mock HTTP live smoke；后续需补 poll -> `/ws` -> publish 的进程级证据。

- 状态：完成
  执行者：main-codex
  范围：ntfy-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 ntfy native adapter 第一阶段的 focused、gateway、类型、全量测试和 whitespace 验证。
  原因：确认 ntfy JSON/JSONL poll normalization、sender allowlist、publish 分片/metadata、doctor availability 和 native runtime 状态红线无回归。
  验证：`cargo fmt --check`；`cargo test ntfy -- --nocapture`（5 passed）；`cargo test gateway -- --nocapture`（45 passed）；`cargo check --all-targets`；`cargo test`（237 passed）；`git diff --check`。
  风险：ntfy mock HTTP live smoke 仍待后续补齐。

- 状态：进行中
  执行者：main-codex
  范围：ntfy-live-smoke-closure
  变动文件：`scripts/ntfy-gateway-smoke.ts`、`package.json`、`src/tui/gateway/channels/runtime.rs`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 `smoke:gateway:ntfy`，用 mock ntfy HTTP server 与 mock `/ws` kernel 验证 poll JSONL -> `gateway.message.send` -> `turn.final` -> publish POST；同时为成功轮询增加默认 1000ms 节流。
  原因：ntfy 单测只能证明 adapter 归一化和 publish 行为，不能证明 daemon 模式下真实 channel runtime、内核 websocket bridge 和 outbound delivery 连通；成功轮询无节流会造成本地/真实 ntfy 端点 tight loop。
  验证：已运行 `npm run smoke:gateway:ntfy`（ok: true，1 GET + 1 POST）与 `cargo test poll_interval_defaults_to_one_second_and_allows_override_value -- --nocapture`；待运行格式、类型、全量测试和 whitespace 验证。
  风险：本 smoke 使用本地 mock kernel，不依赖真实 Flyflor 内核推理；`FLYFLOR_GATEWAY_POLL_INTERVAL_MS` 只作为 runtime env override，后续可继续接 JSONC runtime config。

- 状态：完成
  执行者：main-codex
  范围：ntfy-live-smoke-closure-verification
  变动文件：同上
  摘要：完成 ntfy live smoke、轮询节流 focused test、格式、类型、全量测试和 whitespace 验证。
  原因：确认 ntfy daemon 真实进程链路可从 mock ntfy poll 进入 `/ws` gateway bridge，并把 `turn.final` 通过 HTTP POST publish 回 ntfy topic，同时避免成功轮询 tight loop。
  验证：`npm run smoke:gateway:ntfy`（ok: true，1 GET + 1 POST）；`cargo test poll_interval_defaults_to_one_second_and_allows_override_value -- --nocapture`；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（238 passed）；`git diff --check`。
  风险：本 smoke 使用本地 mock kernel，不依赖真实 Flyflor 内核推理；真实第三方 ntfy server 仍依赖用户 topic/token 环境。

- 状态：进行中
  执行者：main-codex
  范围：matrix-native-channel-adapter
  变动文件：`src/tui/gateway/channels/matrix.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/matrix-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 Matrix Client-Server HTTP native adapter 与 `smoke:gateway:matrix`，完成 `/sync` 入站、`gateway.message.send`、`turn.final`、`m.room.message` 出站的本地 mock 闭环。
  原因：继续推进 western channel 真实 adapter，Matrix 可用本地 homeserver mock 验证进程级 `/ws` 血管层闭环；同时避免把 E2EE、media、reaction、rich formatting 等未完成能力提前宣称 native。
  验证：已运行 `cargo test matrix -- --nocapture`（5 passed）与 `npm run smoke:gateway:matrix`（ok: true）；待运行格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 Matrix plain text HTTP sync/send；E2EE、thread、reaction approval、media/file 后续继续 explicit unavailable。

- 状态：完成
  执行者：main-codex
  范围：matrix-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 Matrix native adapter、mock live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 Matrix 已作为真实 native adapter 接入 registry/doctor/runtime，同时 Discord 等未实现 planned channel 仍保持 explicit unavailable，不假成功。
  验证：`cargo test matrix -- --nocapture`（5 passed）；`npm run smoke:gateway:matrix`（ok: true）；`cargo test gateway -- --nocapture`（51 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（243 passed）；`git diff --check`。
  风险：本轮只实现 Matrix plain text HTTP sync/send；E2EE、thread、reaction approval、media/file 后续继续 explicit unavailable。

- 状态：进行中
  执行者：main-codex
  范围：irc-native-channel-adapter
  变动文件：`src/tui/gateway/channels/irc.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/irc-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 IRC plain TCP native adapter 与 `smoke:gateway:irc`，完成 `PRIVMSG` 入站、`gateway.message.send`、`turn.final`、出站 `PRIVMSG` 的本地 TCP mock 闭环。
  原因：继续推进 longtail/western channel 真实 adapter；IRC 可用本地 TCP server 验证底层协议与 `/ws` 血管层连通，同时保持 TLS、NickServ、SASL、多频道等后续能力不假成功。
  验证：已运行 `cargo test irc -- --nocapture`（5 passed）与 `npm run smoke:gateway:irc`（ok: true）；待运行格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 plain TCP IRC text path；TLS、NickServ、SASL、多频道、mention policy 和 reconnect/backoff 后续继续补。

- 状态：完成
  执行者：main-codex
  范围：irc-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 IRC native adapter、mock TCP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 IRC 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 `PRIVMSG` 入站到 `/ws` 再到出站 `PRIVMSG` 的进程级闭环。
  验证：`cargo test irc -- --nocapture`（5 passed）；`npm run smoke:gateway:irc`（ok: true）；`cargo test gateway -- --nocapture`（56 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（248 passed）；`git diff --check`。
  风险：本轮只实现 plain TCP IRC text path；TLS、NickServ、SASL、多频道、mention policy 和 reconnect/backoff 后续继续补。

- 状态：进行中
  执行者：main-codex
  范围：mattermost-native-channel-adapter
  变动文件：`src/tui/gateway/channels/mattermost.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/mattermost-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 Mattermost REST native adapter 与 `smoke:gateway:mattermost`，完成 posts polling、`gateway.message.send`、`turn.final`、create post 出站的本地 HTTP mock 闭环。
  原因：继续推进 western/longtail channel 真实 adapter；Mattermost REST path 可本地验证 `/ws` 血管层连通，同时避免把 websocket、edit/stream preview、file attachments 等重型能力提前宣称 native。
  验证：已运行 `cargo test mattermost -- --nocapture`（5 passed）与 `npm run smoke:gateway:mattermost`（ok: true）；待运行格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 Mattermost REST polling/send text path；websocket monitor、edit/stream preview、file attachments、mention gating 后续继续补。

- 状态：完成
  执行者：main-codex
  范围：mattermost-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 Mattermost REST native adapter、mock HTTP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 Mattermost 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 posts poll 到 `/ws` 再到 create post reply 的进程级闭环。
  验证：`cargo test mattermost -- --nocapture`（5 passed）；`npm run smoke:gateway:mattermost`（ok: true）；`cargo test gateway -- --nocapture`（61 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（253 passed）；`git diff --check`。
  风险：本轮只实现 Mattermost REST polling/send text path；websocket monitor、edit/stream preview、file attachments、mention gating 和 richer thread behavior 后续继续补。

- 状态：进行中
  执行者：main-codex
  范围：homeassistant-native-channel-adapter
  变动文件：`src/tui/gateway/channels/homeassistant.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/homeassistant-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 Home Assistant native adapter 与 `smoke:gateway:homeassistant`，完成本地 webhook 入站、`gateway.message.send`、`turn.final`、conversation/process 出站的本地 HTTP mock 闭环。
  原因：继续推进 longtail channel 真实 adapter；Home Assistant webhook + REST conversation path 可本地验证 `/ws` 血管层连通，同时避免把 notify/service/entity routing 等家庭自动化能力提前宣称 native。
  验证：待运行 `cargo test homeassistant -- --nocapture`、`npm run smoke:gateway:homeassistant`、格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 webhook ingest 与 conversation/process text path；notify/service/entity routing、event subscription、area/device context 后续继续补。

- 状态：完成
  执行者：main-codex
  范围：homeassistant-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 Home Assistant native adapter、mock HTTP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 Home Assistant 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 webhook event 到 `/ws` 再到 conversation/process reply 的进程级闭环。
  验证：`cargo test homeassistant -- --nocapture`（5 passed）；`npm run smoke:gateway:homeassistant`（ok: true）；`cargo test gateway -- --nocapture`（66 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（258 passed）；`git diff --check`。
  风险：本轮只实现 webhook ingest 与 conversation/process text path；notify/service/entity routing、event subscription、area/device context 后续继续补。

- 状态：进行中
  执行者：main-codex
  范围：openwebui-native-channel-adapter
  变动文件：`src/tui/gateway/channels/openwebui.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/openwebui-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 Open WebUI native adapter 与 `smoke:gateway:open-webui`，完成本地 webhook 入站、`gateway.message.send`、`turn.final`、callback 出站的本地 HTTP mock 闭环。
  原因：继续推进可本地验证的 gateway channel；Open WebUI webhook/callback path 可证明 `/ws` 血管层连通，同时避免把 file/media/plugin schema 等未完成能力提前宣称 native。
  验证：待运行 `cargo test openwebui -- --nocapture`、`npm run smoke:gateway:open-webui`、格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 webhook ingest 与 callback text path；native plugin schema、file upload/download、rich chat metadata 和用户会话映射后续继续补。

- 状态：完成
  执行者：main-codex
  范围：openwebui-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 Open WebUI native adapter、mock HTTP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 Open WebUI 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 webhook payload 到 `/ws` 再到 callback reply 的进程级闭环。
  验证：`cargo test openwebui -- --nocapture`（6 passed）；`npm run smoke:gateway:open-webui`（ok: true）；`cargo test gateway -- --nocapture`（72 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（264 passed）；`git diff --check`。
  风险：本轮只实现 webhook ingest 与 callback text path；native plugin schema、file upload/download、rich chat metadata 和用户会话映射后续继续补。

- 状态：进行中
  执行者：main-codex
  范围：sms-native-channel-adapter
  变动文件：`src/tui/gateway/channels/sms.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/sms-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 SMS/Twilio native adapter 与 `smoke:gateway:sms`，完成 Twilio webhook payload 入站、`gateway.message.send`、`turn.final`、Messages REST 出站的本地 HTTP mock 闭环。
  原因：继续推进 longtail channel 真实 adapter；SMS/Twilio webhook + REST path 可本地验证 `/ws` 血管层连通，同时避免把 signature validation、MMS、delivery callback 等未完成能力提前宣称 native。
  验证：已运行 `cargo test sms -- --nocapture`（6 passed）与 `npm run smoke:gateway:sms`（ok: true）；待运行格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 env payload poll 与 Twilio Messages REST text path；真实 HTTP webhook listener、Twilio signature validation、delivery status callback、MMS media 后续继续补。

- 状态：完成
  执行者：main-codex
  范围：sms-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 SMS/Twilio native adapter、mock HTTP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 SMS 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 Twilio webhook payload 到 `/ws` 再到 Messages REST reply 的进程级闭环。
  验证：`cargo test sms -- --nocapture`（6 passed）；`npm run smoke:gateway:sms`（ok: true）；`cargo test gateway -- --nocapture`（78 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（270 passed）；`git diff --check`。
  风险：本轮只实现 env payload poll 与 Twilio Messages REST text path；真实 HTTP webhook listener、Twilio signature validation、delivery status callback、MMS media 后续继续补。

- 状态：进行中
  执行者：main-codex
  范围：line-native-channel-adapter
  变动文件：`src/tui/gateway/channels/line.rs`、`src/tui/gateway/channels/runtime.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/line-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 LINE native adapter 与 `smoke:gateway:line`，完成 LINE webhook text event 入站、`gateway.message.send`、`turn.final`、reply token POST 出站的本地 HTTP mock 闭环；同时让 runtime 合并 inbound channel anchor metadata。
  原因：继续推进 western/longtail channel 真实 adapter；LINE reply token 是典型平台锚点，必须由 gateway runtime 保留，不能依赖内核原样回传。
  验证：已运行 `cargo test line -- --nocapture`（39 matched passed）、`cargo test outbound_final_merges_inbound_channel_anchor_metadata -- --nocapture`（1 passed）与 `npm run smoke:gateway:line`（ok: true）；待运行格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 env payload poll 与 LINE text reply/push fallback path；真实 HTTP webhook listener、signature validation、rich cards、media download/upload 和 slow response push policy 后续继续补。

- 状态：完成
  执行者：main-codex
  范围：line-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 LINE native adapter、runtime inbound channel anchor metadata 合并、mock HTTP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 LINE 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 LINE webhook text event 到 `/ws` 再到 reply token POST 的进程级闭环；同时守住平台锚点不会在 turn.final 时丢失。
  验证：`cargo test line -- --nocapture`（39 matched passed）；`cargo test outbound_final_merges_inbound_channel_anchor_metadata -- --nocapture`（1 passed）；`npm run smoke:gateway:line`（ok: true）；`cargo test gateway -- --nocapture`（86 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（278 passed）；`git diff --check`。
  风险：本轮只实现 env payload poll 与 LINE text reply/push fallback path；真实 HTTP webhook listener、signature validation、rich cards、media download/upload 和 slow response push policy 后续继续补。

- 状态：进行中
  执行者：main-codex
  范围：bluebubbles-native-channel-adapter
  变动文件：`src/tui/gateway/channels/bluebubbles.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/bluebubbles-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 BlueBubbles/iMessage native adapter 与 `smoke:gateway:bluebubbles`，完成 webhook payload 入站、`gateway.message.send`、`turn.final`、官方 message/text REST 出站的本地 HTTP mock 闭环。
  原因：继续推进 longtail channel 真实 adapter；BlueBubbles REST text path 可本地验证 `/ws` 血管层连通，同时避免把 tapbacks、read receipts、attachments/media 等未完成能力提前宣称 native。
  验证：待运行 `cargo test bluebubbles -- --nocapture`、`npm run smoke:gateway:bluebubbles`、格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 env payload poll 与 BlueBubbles REST text path；真实 HTTP webhook listener、webhook signature、tapbacks、read receipts、attachments/media 后续继续补。

- 状态：完成
  执行者：main-codex
  范围：bluebubbles-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 BlueBubbles/iMessage native adapter、mock HTTP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 BlueBubbles 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 webhook payload 到 `/ws` 再到官方 message/text REST reply 的进程级闭环。
  验证：`cargo test bluebubbles -- --nocapture`（6 passed）；`npm run smoke:gateway:bluebubbles`（ok: true）；`cargo test gateway -- --nocapture`（92 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（284 passed）；`git diff --check`。
  风险：本轮只实现 env payload poll 与 BlueBubbles REST text path；真实 HTTP webhook listener、webhook signature、tapbacks、read receipts、attachments/media、private-api reply threading 和 iMessage availability 后续继续补。

- 状态：进行中
  执行者：main-codex
  范围：email-native-channel-adapter
  变动文件：`src/tui/gateway/channels/email.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/email-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 Email native adapter 与 `smoke:gateway:email`，完成 env JSON 入站、`gateway.message.send`、`turn.final`、plain SMTP DATA 出站的本地 TCP mock 闭环。
  原因：继续推进 longtail channel 真实 adapter；Email 的 env payload + SMTP path 可本地验证 `/ws` 血管层连通，同时避免把 IMAP/TLS/附件/HTML 等未完成能力提前宣称 native。
  验证：待运行 `cargo test email -- --nocapture`、`npm run smoke:gateway:email`、格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 env payload 与 plain SMTP text path；IMAP polling、TLS/STARTTLS、OAuth/app password profiles、HTML stripping、attachment cache 和 thread discovery 后续继续补。

- 状态：完成
  执行者：main-codex
  范围：email-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 Email native adapter、mock SMTP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 Email 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 env JSON message 到 `/ws` 再到 SMTP DATA reply 的进程级闭环。
  验证：`cargo test email -- --nocapture`（5 passed）；`npm run smoke:gateway:email`（ok: true）；`cargo test gateway -- --nocapture`（97 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（289 passed）；`git diff --check`。
  风险：本轮只实现 env payload 与 plain SMTP text path；IMAP polling、TLS/STARTTLS、OAuth/app password profiles、HTML stripping、attachment cache、thread discovery 和 noreply policy 后续继续补。

- 状态：进行中
  执行者：main-codex
  范围：discord-native-channel-adapter
  变动文件：`src/tui/gateway/channels/discord.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/discord-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 Discord REST native adapter 与 `smoke:gateway:discord`，完成 messages poll 入站、`gateway.message.send`、`turn.final`、create message 出站的本地 HTTP mock 闭环。
  原因：继续推进 western channel 真实 adapter；Discord REST polling/send path 可本地验证 `/ws` 血管层连通，同时避免把 Gateway websocket、interactions、components、media、voice 等未完成能力提前宣称 native。
  验证：待运行 `cargo test discord -- --nocapture`、`npm run smoke:gateway:discord`、格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 REST channel messages polling 与 create message text path；Gateway websocket events、slash commands、approval components、typing、edit/stream update、attachments/media 和 voice 后续继续补。

- 状态：完成
  执行者：main-codex
  范围：discord-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 Discord REST native adapter、mock HTTP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 Discord 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 channel messages poll 到 `/ws` 再到 create message reply 的进程级闭环。
  验证：`cargo test discord -- --nocapture`（5 passed）；`npm run smoke:gateway:discord`（ok: true）；`cargo test gateway -- --nocapture`（102 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（294 passed）；`git diff --check`。
  风险：本轮只实现 REST channel messages polling 与 create message text path；Gateway websocket events、slash commands、approval components、typing、edit/stream update、attachments/media、voice 和 richer thread/DM routing 后续继续补。

- 状态：进行中
  执行者：main-codex
  范围：slack-native-channel-adapter
  变动文件：`src/tui/gateway/channels/slack.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/slack-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 Slack Web API native adapter 与 `smoke:gateway:slack`，完成 history polling、`gateway.message.send`、`turn.final`、`chat.postMessage` 出站的本地 HTTP mock 闭环。
  原因：继续推进 western channel 真实 adapter；Slack Web API polling/send path 可本地验证 `/ws` 血管层连通，同时避免把 Socket Mode、Events API signing、blocks/buttons、slash commands、files 等未完成能力提前宣称 native。
  验证：已运行 `cargo test slack -- --nocapture`（6 passed）、`npm run smoke:gateway:slack`（ok: true）与 `cargo test gateway -- --nocapture`（108 passed）；待运行格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 Web API history polling 与 chat.postMessage text path；Socket Mode、Events API signing、blocks/buttons、slash commands、typing、edit/stream update、file upload/download、ephemeral replies 和 DM/channel discovery 后续继续补。

- 状态：完成
  执行者：main-codex
  范围：slack-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 Slack Web API native adapter、mock HTTP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 Slack 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 conversations.history poll 到 `/ws` 再到 chat.postMessage reply 的进程级闭环。
  验证：`cargo test slack -- --nocapture`（6 passed）；`npm run smoke:gateway:slack`（ok: true）；`cargo test gateway -- --nocapture`（108 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（300 passed）；`git diff --check`。
  风险：本轮只实现 Web API history polling 与 chat.postMessage text path；Socket Mode、Events API signing、blocks/buttons、slash commands、typing、edit/stream update、file upload/download、ephemeral replies 和 DM/channel discovery 后续继续补。

- 状态：进行中
  执行者：main-codex
  范围：whatsapp-native-channel-adapter
  变动文件：`src/tui/gateway/channels/whatsapp.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/whatsapp-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 WhatsApp Cloud API native adapter 与 `smoke:gateway:whatsapp`，完成 webhook payload、`gateway.message.send`、`turn.final`、Graph `/messages` 出站的本地 HTTP mock 闭环。
  原因：继续推进 western channel 真实 adapter；WhatsApp Cloud API webhook/send path 可本地验证 `/ws` 血管层连通，同时避免把 Baileys/QR/media/templates/status receipts 等未完成能力提前宣称 native。
  验证：已运行 `cargo test whatsapp -- --nocapture`（5 passed）、`npm run smoke:gateway:whatsapp`（ok: true）与 `cargo test gateway -- --nocapture`（113 passed）；待运行格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 Cloud API env webhook payload 与 direct text send path；真实 HTTP webhook listener、Meta signature validation、status receipts、templates、interactive buttons、media upload/download、group/DM discovery、Baileys child process 和 QR pairing 后续继续补。

- 状态：完成
  执行者：main-codex
  范围：whatsapp-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 WhatsApp Cloud API native adapter、mock HTTP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 WhatsApp 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 webhook payload 到 `/ws` 再到 Graph `/messages` reply 的进程级闭环。
  验证：`cargo test whatsapp -- --nocapture`（5 passed）；`npm run smoke:gateway:whatsapp`（ok: true）；`cargo test gateway -- --nocapture`（113 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（305 passed）；`git diff --check`。
  风险：本轮只实现 Cloud API env webhook payload 与 direct text send path；真实 HTTP webhook listener、Meta signature validation、status receipts、templates、interactive buttons、media upload/download、group/DM discovery、Baileys child process 和 QR pairing 后续继续补。

- 状态：进行中
  执行者：main-codex
  范围：feishu-native-channel-adapter
  变动文件：`src/tui/gateway/channels/feishu.rs`、`src/tui/gateway/channels/mod.rs`、`src/tui/gateway/channels/platform.rs`、`src/tui/gateway/config.rs`、`src/tui/gateway/platforms.rs`、`scripts/feishu-gateway-smoke.ts`、`package.json`、`TODO.md`、`LOGS.md`、`session-table.md`
  摘要：新增 Feishu/Lark Open Platform native adapter 与 `smoke:gateway:feishu`，完成 webhook payload、`gateway.message.send`、`event.publish` card PATCH、`turn.final` card PATCH 的本地 HTTP mock 闭环。
  原因：用户明确点名飞书卡片流式更新等 channel 细节，本轮先落地可验证的 Open Platform text/card update path，同时避免把 approval buttons、slash commands、file/doc/drive、事件签名/加密等未完成能力提前宣称 native。
  验证：已运行 `cargo test feishu -- --nocapture`（5 passed）、`npm run smoke:gateway:feishu`（ok: true）与 `cargo test gateway -- --nocapture`（118 passed）；待运行格式、类型、全量测试和 whitespace 验证。
  风险：本轮只实现 env webhook payload、tenant token、text reply/send 和 interactive card PATCH；真实 HTTP webhook listener、签名/加密、approval buttons、slash commands、文件/文档/云盘、富文本、群入场/ACL 和完整卡片交互后续继续补。

- 状态：完成
  执行者：main-codex
  范围：feishu-native-channel-adapter-verification
  变动文件：同上
  摘要：完成 Feishu/Lark Open Platform native adapter、mock HTTP live smoke、gateway/native planned 红线回归、格式、类型、全量测试和 whitespace 验证。
  原因：确认 Feishu 已作为真实 native adapter 接入 registry/doctor/runtime，并证明 webhook payload 到 `/ws`、runtime event card update、turn final card update 的进程级闭环。
  验证：`cargo test feishu -- --nocapture`（5 passed）；`npm run smoke:gateway:feishu`（ok: true）；`cargo test gateway -- --nocapture`（118 passed）；`cargo fmt --check`；`cargo check --all-targets`；`cargo test`（310 passed）；`git diff --check`。
  风险：本轮只实现 env webhook payload、tenant token、text reply/send 和 interactive card PATCH；真实 HTTP webhook listener、签名/加密、approval buttons、slash commands、文件/文档/云盘、富文本、群入场/ACL 和完整卡片交互后续继续补。
