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

## 2026-05-28 channel parity 与 Codex lane 红线

- 参考项目名只允许出现在历史日志或人工说明中，禁止进入新增源码字段、运行时输出、协议 key、env 前缀或业务 vocabulary；新增源码使用 `source`、`reference`、`channel`、`native_runtime` 等中性命名。
- 所有 channel 名称 canonicalization 只属于配置解析，不能用于意图、路由、记忆、ASK、Scope、结晶等业务语义判断；业务语义继续遵守零字符匹配红线。
- channel identity 只能进入 routing、audit、dedup、reply anchor 和显式 metadata；不能成为 prompt context、Memory owner、Scope owner 或 session。
- `scripts/codex-lanes.sh` 是固定并发入口；子 Codex 必须运行在独立 git worktree + 独立 tmux session 中，禁止在 dirty 主 worktree 直接写实现。
- 查看子 Codex working 细节必须记录在 `session-table.md`：`tmux attach -t <session>` 与 `tmux capture-pane -t <session>:0.0 -p -S -5000`。
- worktree 仅允许软链 `node_modules` 与 `target`；禁止软链运行态 home、日志数据库、账号状态、密钥、`brain.db` 或 `scope.db`。
