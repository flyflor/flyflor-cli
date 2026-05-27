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

## 2026-05-28 固定 Codex Lane

入口：

```bash
npm run codex:lanes
npm run codex:lanes -- --launch-codex
```

查看全部 session：

```bash
tmux list-sessions | rg '^ff-cli-'
```

| Lane | Branch | Worktree Path | Tmux Attach | Capture Working 细节 | Scope |
|---|---|---|---|---|---|
| gateway-core-runtime | `feature/gateway-core-runtime` | `.worktrees/gateway-core-runtime` | `tmux attach -t ff-cli-gateway-core-runtime` | `tmux capture-pane -t ff-cli-gateway-core-runtime:0.0 -p -S -5000` | gateway config、registry、capability、doctor、bridge、unavailable/degraded 语义。 |
| channels-western | `feature/channels-western` | `.worktrees/channels-western` | `tmux attach -t ff-cli-channels-western` | `tmux capture-pane -t ff-cli-channels-western:0.0 -p -S -5000` | Telegram、Discord、Slack、Matrix、WhatsApp、Email、Webhook。 |
| channels-cn | `feature/channels-cn` | `.worktrees/channels-cn` | `tmux attach -t ff-cli-channels-cn` | `tmux capture-pane -t ff-cli-channels-cn:0.0 -p -S -5000` | Feishu、DingTalk、WeCom、WeCom Callback、Weixin、QQBot、Yuanbao。 |
| channels-longtail | `feature/channels-longtail` | `.worktrees/channels-longtail` | `tmux attach -t ff-cli-channels-longtail` | `tmux capture-pane -t ff-cli-channels-longtail:0.0 -p -S -5000` | Google Chat、IRC、ntfy、SimpleX、LINE、Mattermost、Signal、SMS、BlueBubbles、Home Assistant、Open WebUI、Teams、Graph webhook。 |
| tui-ask-layout | `feature/tui-ask-layout` | `.worktrees/tui-ask-layout` | `tmux attach -t ff-cli-tui-ask-layout` | `tmux capture-pane -t ff-cli-tui-ask-layout:0.0 -p -S -5000` | ASK/layout 拆分，不破坏 ASK 菜单视觉样式。 |

依赖软链规则：脚本只软链 `node_modules` 与 `target`；禁止软链运行态 home、账号状态、密钥、日志数据库或 kernel DB。
