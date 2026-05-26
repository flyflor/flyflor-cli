# flyflor-cli 文档

本目录是 `flyflor-cli` Rust TUI 工作区的中英同步文档集。

`flyflor-cli` 是 Flyflor Bun kernel 的外部 thin shell。它渲染 `/ws` snapshots 和 events，发送用户意图与用户决策，并保存本地展示状态。它不是 kernel、ledger、tool executor 或 prompt owner。

## 文档地图

- [架构](architecture.zh.cn.md)：CLI 职责边界、当前源码布局，以及 kernel/CLI ownership 分离。
- [协议](protocol.zh.cn.md)：WebSocket envelope 接线、启动消息、快照、订阅、事件解析和当前闭环缺口。
- [TUI 模型](tui-model.zh.cn.md)：ASK、plan、fork、blackboard、Run timeline、status、右侧面板、热记忆、fork memory 和 tool visibility。
- [开发](development.zh.cn.md)：运行命令、`cargo check`、dev 模式、日志与 tmux 友好的查看方式。

## English Edition

英文文档集保持同样结构：

- [Documentation Index](README.md)
- [Architecture](architecture.md)
- [Protocol](protocol.md)
- [TUI Model](tui-model.md)
- [Development](development.md)

## 当前对齐说明

- Kernel socket 文档用 `ws://127.0.0.1:8788/ws` 作为本地 smoke 示例；CLI 默认值仍是 `ws://127.0.0.1:8787/ws`，除非设置 `FLYFLOR_WS_URL`。
- Kernel 暴露 `server.hello` 和 `capability.catalog.get`；CLI startup 会请求 visible capability catalog。
- Kernel context input 支持 `toolApprovals.mcpToolCalls` 和 `toolApprovals.userToolCalls`；CLI 通过 `/approve` 提供非 YOLO 的单轮 approval，同时保留 YOLO mode 与 tool/run 可见性。

## 同步规则

本集合中每个英文 `.md` 文档都有对应的中文 `.zh.cn.md` 文档。章节顺序与技术口径应保持一致。协议或 UI 行为变化时，必须在同一次变更中更新两种语言版本。

被替代的文档先归档到 `docs/old-docs/`，再重写 active path。
