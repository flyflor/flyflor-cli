# flyflor-cli 文档

本目录是 `flyflor-cli` Rust TUI 工作区的中英同步文档集。

## 文档地图

- [架构](architecture.zh.cn.md)：CLI 职责边界、当前 `src/main.rs` 聚合结构与未来拆分目标。
- [协议](protocol.zh.cn.md)：WebSocket envelope 接线、启动消息、快照、订阅与事件解析。
- [TUI 模型](tui-model.zh.cn.md)：ASK、plan、fork、blackboard、状态、右侧面板、热记忆与 fork memory 展示行为。
- [开发](development.zh.cn.md)：运行命令、`cargo check`、dev 模式、日志与 tmux 友好的查看方式。

## English Edition

英文文档集保持同样结构：

- [Documentation Index](README.md)
- [Architecture](architecture.md)
- [Protocol](protocol.md)
- [TUI Model](tui-model.md)
- [Development](development.md)

## 同步规则

本集合中每个英文 `.md` 文档都有对应的中文 `.zh.cn.md` 文档。章节顺序与技术口径应保持一致。协议或 UI 行为变化时，必须在同一次变更中更新两种语言版本。
