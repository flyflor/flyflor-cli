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

## 2026-05-27 dir-foundation module layout

- [x] 将顶层 `clipboard.rs`、`input.rs`、`shared.rs`、`theme.rs` 移入 `src/tui/`。
- [x] 将顶层 `context/`、`layout/` 移入 `src/tui/context/` 与 `src/tui/layout/`。
- [x] 将 TUI kernel socket module 从 `src/tui/gateway/` 改为 `src/tui/kernel/`，并准备 `src/cli/`、`src/gateway/channels/` roots。
- [x] 复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check` 与 stale path search。

## 2026-05-27 kernel contract audit

- [x] 对齐 kernel socket docs，明确 channel identity 只进入 routing/audit fields，不进入 prompt context。
- [x] 增加 gateway message payload tests，证明 `conversationKey`、`threadId`、`chatType`、`user` 和 gateway metadata 不会创建 `payload.context`。
- [x] 明确 `history.list` 与 read-model snapshots 只用于 query/display，不回灌为 `gateway.message.send` prompt context。
- [x] 复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`。

## 2026-05-27 Gateway JSONC Config

- [x] 增加 CLI-owned `gateway.jsonc` schema，覆盖 core、gateway、streaming、display、platforms。
- [x] 增加 Hermes-compatible channel registry、canonical alias、env alias metadata 和默认 platform config。
- [x] 增加 JSONC parse/init/validate/doctor/channel toggle tests，禁止 session/sessions config fields。
- [x] 将未显式设置 `FLYFLOR_GATEWAY_CHANNELS` 时的 channel 选择回退到默认 JSONC config。

## 2026-05-27 src/tui directory alignment

- [x] 将已合并的 CLI shell、gateway runtime、gateway channels、gateway JSONC config 和 platform registry 统一迁入 `src/tui`。
- [x] `src/main.rs` 只挂载 `mod tui`，通过 `tui::cli` 与 `tui::gateway` 使用功能入口。
- [x] 复跑 `cargo fmt --check`、`cargo check`、`cargo test`、`git diff --check`，确认 `src/tui` 内结构可用。

## 2026-05-27 gateway bridge streaming

- [x] inbound normalized channel message 映射为 `gateway.message.send`，保留 routing/audit/reply anchor 与显式 `payload.context`。
- [x] ASK/approval metadata 作为结构化 metadata/context 透传，不把工具授权写成普通用户文本。
- [x] outbound 消费 `turn.delta`、`turn.final`、`turn.error` 与 subscribed `event.publish`，按 channel capability 选择 typing/send 或 edit/draft/card stream update。
- [x] channel capability report 显式标记 `available`/`degraded`/`unavailable`，未实现平台不返回假成功。
- [x] 增加 mock WS targeted tests 覆盖 inbound envelope、ASK/approval metadata、delta/final/error 和 event.publish streaming。

## 2026-05-27 src/tui external tool failure visibility

- [x] Timeline parser 提取外部工具 nested failure/unavailable 字段，避免 browser/computer sidecar 错误变成 raw JSON 或空白 detail。
- [x] Subagent tree 同步显示 nested failure detail，让 Exo/child 工具失败在展开区可见。
- [x] 增加 targeted parser tests，覆盖 `result.response.error/reason/code` 形态且禁止 `unknown`/raw JSON 回退。

## 2026-05-28 全 channel registry 与 Codex lane 固化

- [x] 新增中性命名的全 channel registry 元数据，覆盖 Telegram、Discord、Slack、Matrix、WhatsApp、Feishu、DingTalk、WeCom、WeCom Callback、Weixin、QQBot、Email、Webhook、Teams、Microsoft Graph webhook、Google Chat、IRC、ntfy、SimpleX、LINE、Mattermost、Signal、SMS、BlueBubbles、Home Assistant、Open WebUI、Yuanbao。
- [x] 每个 channel 记录 required env、optional env、source channel、capability feature、细节标签与 `native/planned` runtime 状态；未原生实现的平台仍必须 explicit unavailable，不允许假成功。
- [x] 新增源码红线：新增源码、协议 key、运行时输出和 env 前缀不使用参考项目关键字；历史日志中的旧词仅作为历史记录保留。
- [x] 新增 `scripts/codex-lanes.sh`，固定 `gateway-core-runtime`、`channels-western`、`channels-cn`、`channels-longtail`、`tui-ask-layout` 五个 worktree/tmux lane，并自动软链 `node_modules` 与 `target`。
- [ ] 逐 lane 落地真实 channel adapter：先补 generic core runtime tests，再分 western、CN、longtail adapters；每个 adapter 至少覆盖 config doctor、missing credential unavailable、inbound normalization、outbound send、ASK/approval metadata。
- [ ] 把 TUI 目录继续按 `progress/components/layout/context/bulletin_board` 拆分，但不改 ASK 菜单视觉样式。

## 2026-05-28 gateway channel doctor core

- [x] 增加 `flyflor gateway channel doctor <name>` 单通道诊断入口，输出 `native/planned`、`available/unavailable`、required/missing env、features 和 details。
- [x] `config doctor` 输出补充 availability，不改变已有 enabled-channel 诊断入口。
- [x] 增加全 27 channel 缺失 required env 时均为 explicit unavailable 的 generic core tests；planned channel 即使 env 存在也不能假装 available。
- [ ] 继续按 lane 落地真实 adapter；本轮只闭合 generic doctor/status 契约，不宣称 western、CN、longtail adapter 已完成。

## 2026-05-28 live TUI send closure

- [x] 修复 tmux 场景下普通 Enter 被识别成 Control+Enter 后只换行的问题，保证真实 TUI smoke 能提交消息。
- [x] 加强 `smoke:live:tui`，等待 CLI socket connected 后再驱动输入，并强制断言 kernel log 出现 `gateway.message.send` 与 `mcp.tool.call.executed`。
- [x] 复跑 `cargo fmt --check && cargo test --quiet` 与真实 `npm run smoke:live:tui`，保留报告目录 `.flyflor-cli/live/2026-05-27T17-24-11-303Z/`。
- [x] 后续 TUI 需要区分 `Confirm` 与 `ASK`：高风险工具授权只渲染 Confirm，不进入 ASK 结晶样式；ASK 答案发送后等待内核自动续跑。

## 2026-05-28 Confirm / ASK Display Split

- [x] 公民权限/高风险授权菜单标题渲染为 `Confirm 授权执行策略`，普通 ASK 菜单仍渲染为 `ASK`。
- [x] 保留结构化 metadata 发送，不把 `continue-tools` 等授权 token 写成普通用户文本。
- [x] 公民权限/高风险授权新增 `metadata.confirmAnswer`；普通 ASK 仍只走 `askAnswer`。
- [x] 内核提供独立 Confirm snapshot/event 后，公民权限发送路径已移除 ASK-compatible `metadata.askAnswer` fallback。

## 2026-05-28 Confirm Event Timeline

- [x] `event.subscribe` 固定列表订阅 `confirm.answered`。
- [x] Run timeline 将 `confirm.answered` 渲染为 `Confirm answered`，不混入 ASK crystallization 样式。
- [x] Subagent tree 将 `confirm.answered` 用作 pending needs-user marker 的闭合信号。
- [x] 启动 bootstrap 请求 `confirm.list`，并把 `confirm.snapshot` 恢复为 Run timeline Confirm row。
- [x] 接入 Confirm read-model owner 后，移除公民权限发送路径的 ASK-compatible permission fallback。

## 2026-05-28 Confirm Component Foundation

- [x] 新增 `src/tui/confirm/` owner，集中解析 `confirm.snapshot` read-model 并持有 `ConfirmState`。
- [x] `confirm.snapshot` 先进入 Confirm read-model state，再投影为 `confirm.answered` timeline row，不再由 `main.rs` 手写 ASK-compatible pseudo events。
- [x] targeted tests 覆盖 Confirm snapshot 恢复不会生成 ASK continuation row，也不会携带 `askAnswer` 结晶入口。
- [x] 公民权限 ASK-compatible metadata fallback 已从 CLI 发送路径移除；内核仍可接受旧客户端 fallback。

## 2026-05-28 Telegram Native Channel Adapter

- [x] 新增 Telegram Bot API native adapter，接入 channel registry，不再把 Telegram 标记为 planned runtime。
- [x] Telegram inbound `getUpdates` normalization 覆盖 private/group chat、用户显示名、source message metadata、media unavailable notice 和 dedup。
- [x] Telegram outbound 覆盖 `sendMessage`、`sendChatAction` 与 explicit media unavailable；stream edit 只作为 adapter 能力实现，runtime 仍需后续补 reply anchor 后再启用。
- [x] Gateway doctor 测试覆盖 Telegram token 存在时可用，planned channel 改由 Discord 继续验证“即使 env 存在也不能假成功”。
- [x] 收紧 Telegram capability report：当前只声明 send/typing 可用，edit/card/draft/media 均显式 unavailable，避免 runtime 在没有 bot message id anchor 前误走 streaming edit。
- [x] Gateway runtime 为 edit-capable channel 增加先发占位消息并保存 bot message id 的 stream route anchor，Telegram 可声明 edit streaming。
- [ ] 后续补 Telegram 真实 Bot API sandbox smoke，验证 delta -> placeholder send -> editMessageText -> final edit 的真实网络路径。
- [ ] 继续按 lane 落地 Discord、Slack、Matrix、Email、Webhook 等 western adapters；每个 adapter 仍需覆盖 config doctor、missing credential unavailable、inbound normalization、outbound send、ASK/approval metadata。

## 2026-05-28 Webhook Native Channel Adapter

- [x] 新增 Webhook native adapter，使用本地 HTTP POST listener 接收入站 JSON，并通过现有 `/ws` gateway bridge 送入内核。
- [x] Webhook 入站验证 `WEBHOOK_SECRET` / Bearer secret，支持 `WEBHOOK_ALLOWED_SOURCES`、context 透传、metadata 透传和 direct/group route normalization。
- [x] Webhook outbound 使用 `WEBHOOK_PUBLIC_URL` callback 发送结构化 reply payload；未配置 callback 时 send capability 为 degraded，调用返回 explicit unavailable。
- [x] Gateway registry/doctor 将 Webhook 标记为 native，并新增测试守住 native runtime 仅包含 Telegram、Weixin、Webhook，避免 planned channel 假成功。
- [x] 接通 `flyflor gateway run` 启动 channel runtime，并新增 `smoke:gateway:webhook` 验证 HTTP POST -> `gateway.message.send` -> `turn.final` -> callback。

## 2026-05-28 ntfy Native Channel Adapter

- [x] 新增 ntfy native adapter，支持 `/topic/json?poll=1` JSON/JSONL 入站解析与 HTTP POST publish 出站。
- [x] ntfy 入站 normalization 覆盖 topic route、sender allowlist、title/priority metadata 和 non-message event 过滤。
- [x] ntfy outbound 覆盖 4096 字符分片、token header、explicit media unavailable 和 curl error 分类。
- [x] Gateway registry/doctor 将 ntfy 标记为 native，并新增测试守住 native runtime 列表包含 Telegram、Weixin、Webhook、ntfy。
- [x] 后续补 ntfy 本地 mock HTTP smoke：poll JSONL -> `gateway.message.send` -> `turn.final` -> publish POST。

## 2026-05-28 Matrix Native Channel Adapter

- [x] 新增 Matrix Client-Server HTTP native adapter，支持 `/sync` 入站和 `m.room.message` 出站。
- [x] Matrix 入站 normalization 覆盖 room route、sender allowlist、self-message filter、source event metadata 和 plain text body。
- [x] Matrix outbound 覆盖 room path encoding、send transaction id、typing indicator、text chunking、explicit media unavailable 和 Matrix error 分类。
- [x] Gateway registry/doctor 将 Matrix 标记为 native，并收紧 capability 只声明当前真实可用的 text/typing/polling/group chat。
- [x] 新增 Matrix 本地 mock HTTP smoke：sync event -> `gateway.message.send` -> `turn.final` -> send PUT。
- [ ] 后续补 Matrix E2EE、rich formatting、thread、reaction approval 和 media/file 能力；这些能力在当前 adapter 中仍保持 explicit unavailable。

## 2026-05-28 IRC Native Channel Adapter

- [x] 新增 IRC plain TCP native adapter，支持 NICK/USER/JOIN、PING/PONG、PRIVMSG 入站和出站。
- [x] IRC 入站 normalization 覆盖 channel/DM route、nick/prefix sender、allowlist、source message metadata 和 self-message filter。
- [x] IRC outbound 覆盖 channel/DM target、line chunking、explicit media unavailable 和 TCP read/write error 分类。
- [x] Gateway registry/doctor 将 IRC 标记为 native，并收紧 capability 只声明当前真实可用的 plain text group chat。
- [x] 新增 IRC 本地 mock TCP smoke：PRIVMSG -> `gateway.message.send` -> `turn.final` -> outbound PRIVMSG。
- [ ] 后续补 IRC TLS、NickServ、SASL、多频道、mention policy 和更完整 reconnect/backoff；这些能力当前仍保持 explicit unavailable 或未声明。

## 2026-05-28 Mattermost Native Channel Adapter

- [x] 新增 Mattermost REST native adapter，支持 channel posts polling 和 create post 出站。
- [x] Mattermost 入站 normalization 覆盖 channel route、user allowlist、root/thread metadata、source post id 和 create_at cursor。
- [x] Mattermost outbound 覆盖 create post、reply root_id、text chunking、explicit media unavailable 和 REST error 分类。
- [x] Gateway registry/doctor 将 Mattermost 标记为 native，并收紧 capability 只声明当前真实可用的 REST polling/group text。
- [x] 新增 Mattermost 本地 mock HTTP smoke：posts poll -> `gateway.message.send` -> `turn.final` -> create post。
- [ ] 后续补 Mattermost websocket monitor、edit/stream preview、file attachments、mention gating 和 richer thread behavior；这些能力当前仍保持 explicit unavailable 或未声明。

## 2026-05-28 Home Assistant Native Channel Adapter

- [x] 新增 Home Assistant native adapter，支持本地 webhook 入站和 `/api/conversation/process` 出站。
- [x] Home Assistant 入站 normalization 覆盖 conversation route、user allowlist、nested event payload、source message id 和 channel metadata。
- [x] Home Assistant outbound 覆盖 conversation_id 续接、Bearer token、explicit media unavailable 和 REST error 分类。
- [x] Gateway registry/doctor 将 Home Assistant 标记为 native，并收紧 capability 只声明当前真实可用的 webhook ingest / conversation text。
- [x] 新增 Home Assistant 本地 mock HTTP smoke：webhook event -> `gateway.message.send` -> `turn.final` -> conversation/process。
- [ ] 后续补 Home Assistant notify/service/entity routing、event subscription、area/device context 和更完整 conversation response 映射；这些能力当前仍保持 explicit unavailable 或未声明。

## 2026-05-28 Open WebUI Native Channel Adapter

- [x] 新增 Open WebUI native adapter，支持本地 webhook 入站和 `OPEN_WEBUI_PUBLIC_URL` callback 出站。
- [x] Open WebUI 入站 normalization 覆盖 chat route、nested message payload、user allowlist、context 透传和 metadata 透传。
- [x] Open WebUI outbound 覆盖 callback reply、secret header、explicit media unavailable 和 missing callback degraded/unavailable。
- [x] Gateway registry/doctor 将 Open WebUI 标记为 native，并收紧 capability 只声明当前真实可用的 webhook ingest / callback text。
- [x] 新增 Open WebUI 本地 mock HTTP smoke：webhook payload -> `gateway.message.send` -> `turn.final` -> callback。
- [ ] 后续补 Open WebUI native plugin schema、file upload/download、rich chat metadata 和用户会话映射；这些能力当前仍保持 explicit unavailable 或未声明。

## 2026-05-28 SMS Native Channel Adapter

- [x] 新增 SMS/Twilio native adapter，支持 Twilio webhook payload 入站和 Messages REST 出站。
- [x] SMS 入站 normalization 覆盖 JSON/form payload、phone allowlist、direct route、source message id 和 channel metadata。
- [x] SMS outbound 覆盖 Twilio basic auth、From/To/Body form send、message chunking、REST error 分类和 explicit media unavailable。
- [x] Gateway registry/doctor 将 SMS 标记为 native，并收紧 capability 只声明当前真实可用的 direct text / webhook payload。
- [x] 新增 SMS 本地 mock HTTP smoke：Twilio webhook payload -> `gateway.message.send` -> `turn.final` -> Messages REST POST。
- [ ] 后续补真实 HTTP webhook listener、Twilio signature validation、delivery status callback、MMS media 和多短信会话策略；这些能力当前仍保持 explicit unavailable 或未声明。
