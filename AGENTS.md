# Flyflor CLI Agent Rules

本仓库是 `flyflor-cli` TUI/thin client。所有自动化开发代理必须遵守以下红线。

- 本 worktree 只处理 `flyflor-cli` 侧 ASK 菜单、公民权限展示、Exo timeline、history/fork/read-model UI 和 socket payload 消费。
- `docs-guardrails` lane 只更新项目 guardrails、README/docs、TODO/LOGS 等文档闭环；除非文档工具链需要，禁止实现 feature code。
- 禁止修改 `flyflor` 内核、`flyflor-front`、`reference` 和无关项目。
- TUI 只能通过 socket/gateway control/event 与内核交互，禁止直接写 `brain.db`、`scope.db` 或运行态日志数据库。
- ASK 推荐项只能高亮，不能自动提交；普通输入不能被 pending ASK 劫持成默认选择。
- 公民权限授权必须作为结构化 metadata 发送，禁止把 `continue-tools`、`keep-budget`、`keep-subagents` 等选择写成普通用户消息。
- Exo timeline 禁止显示 `unknown`；等待权限、运行、完成、失败都必须有明确状态。
- 最后一个 Exo 自动展开，其余默认折叠；detail 请求必须去重或节流，避免 socket 噪声。
- 约定大于配置；目录和文件名表达 owner，代码可以重复，但不得抽无 owner 的万能 helper。
- Rust TUI 代码优先沿用现有 module/state/parser/view 边界，避免面向过程函数泛滥。
- 修改前读 `TODO.md`，修改后追加 `LOGS.md`；`TODO.md` 只允许修改状态和追加内容，`LOGS.md` 只允许追加。

常用验证：

```bash
cargo check
cargo test
```
