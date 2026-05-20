# flyflor-cli Todo

## 当前状态

- `flyflor-cli` 已经不再只是 mock TUI。
- Rust 侧已经接入一条真实主链路：
  - CLI 解析模式
  - managed-local 模式解析并启动本地 `flyflor` binary
  - 等待 `http://127.0.0.1:8787/health`
  - 连接 `ws://127.0.0.1:8787/ws`
  - 收取 `server.hello` / `ack` / `turn.delta` / `turn.final`
  - 将 turn / planning / ask / loop 投影给现有 TUI
- `cargo check` 已通过。
- `cargo run --bin flyflor -- --help` 已通过。
- websocket 实测已跑通一轮真实输入，确认收到了：
  - `server.hello`
  - `ack`
  - `turn.delta`
  - `turn.final`

## 当前入口约定

- Rust CLI 的对外命令面已经改成 `flyflor`。
- 当前 crate 包名仍然是 `flyflor-cli`，但已通过 `[[bin]]` 暴露 `flyflor` binary。
- 目标产品形态：
  - `flyflor -h` 显示 Rust CLI help
  - `flyflor` 默认进入 Rust TUI
- 当前 Bun binary 的现实行为已经验证过：
  - `flyflor -h` 不会显示 help，而是直接进入 chat
  - `flyflor gateway -h` 也不会只显示帮助，而是会真实启动 gateway
- 结论：
  - 最终对外入口必须由 Rust CLI 接管
  - Bun `flyflor` 只适合作为被托管的 agent kernel binary

## 当前架构

- `src/protocol.rs`
  - Rust 侧 WS envelope / payload 类型
- `src/gateway_core.rs`
  - websocket client
  - 连接状态
  - snapshot cache
  - event 分发
- `src/cli.rs`
  - CLI 参数解析
  - binary resolver
  - 本地 kernel process 启动
  - health check
- `src/runtime.rs`
  - process state + connection state 聚合
  - turn / ask / planning / loop 投影
  - 右侧面板状态组装
- `src/main.rs`
  - 现有 TUI 主循环
  - mock/live 模式切换

## 已完成内容

- 已新增 install-aware binary resolver，优先级如下：
  1. `--binary <path>`
  2. `FLYFLOR_BINARY`
  3. `~/.flyflor/dist/flyflor`
  4. dev fallback：当前工作目录下 `flyflor/dist/flyflor`
- 已支持三种模式：
  - `Mock`
  - `ManagedLocalBinary`
  - `RemoteWs`
- 已接通 managed-local 默认主链路：
  - spawn `<resolved-binary> gateway`
  - wait `/health`
  - connect `/ws`
- 已让 TUI 主循环消费 runtime bridge，而不是只生成 mock turn。
- 已把右侧面板接上真实状态：
  - mode
  - binary path
  - endpoint
  - active request
  - subscriptions
  - process status
- 已修复一个重要投影问题：
  - ask / planning / loop 不会在 active turn 结束后立刻丢失
  - 当前会保留最近一次 `turn.final.reply.metadata` 权威快照

## 协议对接注意项

- `flyflor` 的 `/ws` 必须保持简单通用，不能为 TUI 打私有补丁协议。
- Rust UI 不从 `reply.text` 猜语义，只读结构化 metadata。
- 当前轮权威状态只读这些地方：
  - ask: `turn.final.reply.metadata.ask`
  - planning: `turn.final.reply.metadata.planning`
  - loop: `turn.final.reply.metadata.executiveToolLoop`
  - ask 下的 loop fallback: `turn.final.reply.metadata.ask.executiveToolLoop`
- 连接级只读 snapshot 只看这些地方：
  - `server.hello`
  - `gateway.status.snapshot`
  - `capability.catalog.snapshot`
  - `ack`
- `event.publish` 目前只应被当作时间线 / 审计 / 提示流，不应反向写成当前轮 ask/planning/loop 的权威状态。
- 不能把连接级 snapshot 当成 turn 结果，也不能把 turn metadata 回灌成 connection state 事实。

## 当前保守处理

- managed-local 目前只允许：
  - host: `127.0.0.1`
  - port: `8787`
- 原因不是 Rust 侧做不到，而是当前 Bun `flyflor` binary 尚未提供稳定、已验证的 host/port 注入契约。
- 这里故意保守：
  - 宁可显式报错
  - 不做“表面支持 `--host/--port`，实际没有真正传给 kernel”的假兼容

## 当前问题 / 未完成项

- 还没有完成“人在 TUI 里手动输入一轮并肉眼确认 UI 流式更新”的正式验收。
- `gateway_core` 现在是最小 plain `ws://` 路径优先：
  - 当前本地链路稳定
  - 未来如果要认真支持 `wss://`，需要重新审视 stream 配置方式
- 还没有实现真正的 reconnect/backoff 生命周期。
- 还没有把 process stderr tail 真正采集并显示到状态面板。
- 还没有把 runtime event 渲染进时间线或侧边提示。
- 还没有梳理 CLI / TUI / gateway / channel 的后续子命令或子模式。
- README 仍停留在 mock 原型描述，已过时。

## TUI 对接注意项

- 不要随意改现有 TUI 布局和 bubble 样式。
- 当前 TUI 有很多已经做好的兼容处理，尤其要谨慎：
  - 滚动
  - 复制
  - 选择
  - resize
  - 右侧 panel 的 scroll ownership
- 不要把 runtime 接入写成“为 live 模式另做一套 UI”。
- 正确方向是：
  - 保持现有 TUI 结构
  - 只替换其数据来源和状态投影

## CLI 层注意项

- 路径规则必须集中在 resolver，不能散落到 TUI 或 `gateway_core`。
- `~/.flyflor/dist/flyflor` 是当前安装态主路径。
- dev fallback 只是阶段性兜底，不是长期安装契约。
- 未来 `npm i -g flyflor` 时，Rust CLI 才应该成为用户真正执行的 `flyflor`。
- Bun binary 需要退到被 CLI 托管的位置，例如安装目录下的 kernel binary。

## 下一步计划

### Phase 1 收尾

- 完成 TUI live 手动验收：
  - 输入真实消息
  - 确认 `turn.delta`
  - 确认 `turn.final`
  - 确认 planning / ask / loop 在右侧正确投影
- 完成 managed-local 生命周期补强：
  - child exit 观察
  - health timeout 错误面
  - connection degraded 显示
  - stderr tail 采集
- 清理 README，使其不再误导为 mock-only 项目。

### Phase 2

- 把 CLI 层正式拉开：
  - help / default entry
  - mock / managed-local / remote-ws
  - 后续 ask / planning / gateway / channel / debug 命令面
- 把 gateway_core 的 reconnect / ping / refresh / subscribe 生命周期补完整。
- 把 runtime events 以“提示流”而非“权威状态”方式接入 UI。

### Phase 3

- 规划 npm 全局安装：
  - `npm i -g flyflor`
  - Rust CLI 作为外部入口
  - Bun flyflor 作为内核 binary
  - resolver 适配安装产物布局

## 验证记录

- 已验证：
  - `cargo check`
  - `cargo run --bin flyflor -- --help`
  - 直接连接 `ws://127.0.0.1:8787/ws`
  - 一轮真实 `gateway.message.send`
  - 收到真实 `turn.delta`
  - 收到真实 `turn.final`
- 已确认当前 Bun binary 的限制：
  - `flyflor -h` 不等于 help
  - `flyflor gateway -h` 不等于 help

## 不要做的事

- 不要为了 TUI 接通去给 `flyflor` WS 协议打补丁。
- 不要从 reply 文本猜 ask / planning / loop。
- 不要把 dev fallback 路径当成正式安装契约。
- 不要在多个层里重复写路径推断。
- 不要在还没确认 Bun kernel 参数契约时，假装支持 host/port 自定义。
