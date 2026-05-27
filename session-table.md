# Flyflor CLI Workmux Session Table

本文件记录协调者创建的 workmux / tmux / Codex 子智能体入口。所有 lane 都是独立 tmux session，便于多终端并行查看完整交互。

## 总览命令

- Dashboard: `workmux dashboard -s --preview-size 70`
- Worktree list: `workmux list`
- Session list: `tmux list-sessions | rg 'wm-'`
- Coordinator session: `tmux attach -t flyflor-cli-coordinator`
- Tmux capture template: `tmux capture-pane -t <session>:0.0 -p -S -200`
- Workmux path template: `workmux path <handle>`
- Send template: `workmux send <handle> "<message>"`

## Sessions

| Lane | Branch | Handle | Worktree Path | Tmux Attach | Capture | Send | Scope |
|---|---|---|---|---|---|---|---|
| docs-guardrails | `feature/docs-guardrails` | `docs-guardrails` | `/Users/yi./Desktop/yi/flyflors/flyflor-cli/.worktrees/docs-guardrails` | `tmux attach -t wm-docs-guardrails` | `tmux capture-pane -t wm-docs-guardrails:0.0 -p -S -200` | `workmux send docs-guardrails "<message>"` | 更新红线、TODO/LOGS 追加规则、README/docs 一致性。 |
| main-rs-split | `feature/main-rs-split` | `main-rs-split` | `/Users/yi./Desktop/yi/flyflors/flyflor-cli/.worktrees/main-rs-split` | `tmux attach -t wm-main-rs-split` | `tmux capture-pane -t wm-main-rs-split:0.0 -p -S -200` | `workmux send main-rs-split "<message>"` | 机械拆分 `src/main.rs`，不改变 TUI 行为。 |
| cli-shell | `feature/cli-shell` | `cli-shell` | `/Users/yi./Desktop/yi/flyflors/flyflor-cli/.worktrees/cli-shell` | `tmux attach -t wm-cli-shell` | `tmux capture-pane -t wm-cli-shell:0.0 -p -S -200` | `workmux send cli-shell "<message>"` | 实现 `flyflor -h`、`flyflor gateway -h`、默认进入 TUI 的 CLI 壳。 |
| gateway-runtime | `feature/gateway-runtime` | `gateway-runtime` | `/Users/yi./Desktop/yi/flyflors/flyflor-cli/.worktrees/gateway-runtime` | `tmux attach -t wm-gateway-runtime` | `tmux capture-pane -t wm-gateway-runtime:0.0 -p -S -200` | `workmux send gateway-runtime "<message>"` | 实现 CLI 侧 gateway daemon 生命周期、pid/lock/status/logs、Flyflor `/ws` 桥接。 |
| gateway-channels | `feature/gateway-channels` | `gateway-channels` | `/Users/yi./Desktop/yi/flyflors/flyflor-cli/.worktrees/gateway-channels` | `tmux attach -t wm-gateway-channels` | `tmux capture-pane -t wm-gateway-channels:0.0 -p -S -200` | `workmux send gateway-channels "<message>"` | 移植 Hermes channel 抽象，优先完整实现 Weixin iLink。 |
| npm-release | `feature/npm-release` | `npm-release` | `/Users/yi./Desktop/yi/flyflors/flyflor-cli/.worktrees/npm-release` | `tmux attach -t wm-npm-release` | `tmux capture-pane -t wm-npm-release:0.0 -p -S -200` | `workmux send npm-release "<message>"` | npm 全局安装、二进制包装、cross-build/release smoke。 |

## Merge Order

1. `docs-guardrails`
2. `main-rs-split`
3. `cli-shell`
4. `gateway-runtime`
5. `gateway-channels`
6. `npm-release`
