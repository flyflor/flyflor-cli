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

## 2026-05-28 本轮主控真实交互记录

主控没有启动新的实现型子 Codex；沿用固定 lane 表和现有 preview tmux session。查看命令：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core)-'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
tmux capture-pane -t ff-core-preview-kernel-tool-loop:0.0 -p -S -5000
```

本轮真实 TUI smoke 证据：

```bash
npm run smoke:live:tui
cat .flyflor-cli/live/2026-05-27T17-24-11-303Z/report.json
rg 'gateway.message.send|mcp.tool.call.executed|turn.error' .flyflor-cli/live/2026-05-27T17-24-11-303Z/kernel.log
```

复检后的最新 TUI smoke 证据：

```bash
cat .flyflor-cli/live/2026-05-27T17-34-31-376Z/report.json
rg 'gateway.message.send|mcp.tool.call.executed|turn.error' .flyflor-cli/live/2026-05-27T17-34-31-376Z/kernel.log
```

## 2026-05-28 Confirm / ASK 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Confirm/ASK 显示切片。沿用既有 lane 表，仍可用下列命令查看现有 session 或历史工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
tmux capture-pane -t ff-core-preview-kernel-tool-loop:0.0 -p -S -5000
```

## 2026-05-28 Confirm Event Timeline 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成内核 `confirm.answered` 事件与 CLI timeline 消费闭环。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
tmux capture-pane -t ff-core-preview-kernel-tool-loop:0.0 -p -S -5000
```

## 2026-05-28 Confirm Read Model 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 CLI `confirm.list` bootstrap 与 `confirm.snapshot` read-model 消费。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
tmux capture-pane -t ff-core-preview-kernel-tool-loop:0.0 -p -S -5000
```

## 2026-05-28 Confirm Component Foundation 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 CLI Confirm read-model owner 拆分。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
tmux capture-pane -t ff-core-preview-kernel-tool-loop:0.0 -p -S -5000
```

## 2026-05-28 Telegram Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Telegram native adapter 第一阶段。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
```

## 2026-05-28 Gateway Edit Stream Route Anchor 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 edit-capable channel 的占位消息与 bot message id anchor。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
```

## 2026-05-28 Webhook Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Webhook native adapter 第一阶段。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
```

## 2026-05-28 Webhook Live Smoke Closure 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Webhook live smoke 与 gateway runtime channel 启动接线。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
npm run smoke:gateway:webhook
```

## 2026-05-28 ntfy Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 ntfy native adapter 第一阶段。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test ntfy -- --nocapture
```

## 2026-05-28 ntfy Live Smoke Closure 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 ntfy live smoke 与 channel runtime 成功轮询节流。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
npm run smoke:gateway:ntfy
```

## 2026-05-28 Matrix Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Matrix native adapter 与本地 mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test matrix -- --nocapture
npm run smoke:gateway:matrix
```

## 2026-05-28 IRC Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 IRC native adapter 与本地 TCP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test irc -- --nocapture
npm run smoke:gateway:irc
```

## 2026-05-28 Mattermost Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Mattermost REST native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test mattermost -- --nocapture
npm run smoke:gateway:mattermost
```

## 2026-05-28 Gateway Channel Smoke Closure Audit 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 补齐 Telegram/Weixin smoke 并完成 27 channel native/smoke 对齐审计。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test telegram -- --nocapture
cargo test weixin -- --nocapture
npm run smoke:gateway:telegram
npm run smoke:gateway:weixin
cargo run --quiet -- gateway config list
rg --files scripts | rg 'gateway-smoke\.ts$' | wc -l
rg -n '"smoke:gateway:' package.json | wc -l
```

## 2026-05-28 Home Assistant Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Home Assistant native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test homeassistant -- --nocapture
npm run smoke:gateway:homeassistant
```

## 2026-05-28 Open WebUI Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Open WebUI native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test openwebui -- --nocapture
npm run smoke:gateway:open-webui
```

## 2026-05-28 SMS Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 SMS/Twilio native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test sms -- --nocapture
npm run smoke:gateway:sms
```

## 2026-05-28 LINE Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 LINE native adapter、runtime inbound channel anchor metadata 合并与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test line -- --nocapture
cargo test outbound_final_merges_inbound_channel_anchor_metadata -- --nocapture
npm run smoke:gateway:line
```

## 2026-05-28 BlueBubbles Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 BlueBubbles/iMessage native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test bluebubbles -- --nocapture
npm run smoke:gateway:bluebubbles
```

## 2026-05-28 Email Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Email native adapter 与本地 SMTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test email -- --nocapture
npm run smoke:gateway:email
```

## 2026-05-28 Discord Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Discord REST native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test discord -- --nocapture
npm run smoke:gateway:discord
```

## 2026-05-28 Slack Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Slack Web API native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test slack -- --nocapture
npm run smoke:gateway:slack
```

## 2026-05-28 WhatsApp Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 WhatsApp Cloud API native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test whatsapp -- --nocapture
npm run smoke:gateway:whatsapp
```

## 2026-05-28 Feishu Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Feishu/Lark Open Platform native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test feishu -- --nocapture
npm run smoke:gateway:feishu
```

## 2026-05-28 DingTalk Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 DingTalk OpenAPI native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test dingtalk -- --nocapture
npm run smoke:gateway:dingtalk
```

## 2026-05-28 QQBot Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 QQBot Official API v2 native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test qqbot -- --nocapture
npm run smoke:gateway:qqbot
```

## 2026-05-28 Signal Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Signal signal-cli REST native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test signal -- --nocapture
npm run smoke:gateway:signal
```

## 2026-05-28 WeCom Callback Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 WeCom Callback Corp API native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test wecom_callback -- --nocapture
npm run smoke:gateway:wecom-callback
```

## 2026-05-28 WeCom Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 WeCom AI Bot native adapter 与本地 WebSocket mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test wecom -- --nocapture
npm run smoke:gateway:wecom
```

## 2026-05-28 Google Chat Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Google Chat native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test google_chat -- --nocapture
npm run smoke:gateway:google-chat
```

## 2026-05-28 MSGraph Webhook Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Microsoft Graph webhook native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test msgraph -- --nocapture
npm run smoke:gateway:msgraph-webhook
```

## 2026-05-28 SimpleX Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 SimpleX Chat native adapter 与本地 WebSocket mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test simplex -- --nocapture
npm run smoke:gateway:simplex
```

## 2026-05-28 Teams Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Microsoft Teams native adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test teams -- --nocapture
npm run smoke:gateway:teams
```

## 2026-05-28 Yuanbao Channel Adapter 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Yuanbao native bridge adapter 与本地 HTTP mock live smoke。沿用固定 lane 表，当前可用下列命令查看历史 session 或 preview 工作细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
tmux capture-pane -t ff-cli-preview-gateway-core-runtime:0.0 -p -S -5000
tmux capture-pane -t ff-cli-preview-tui-ask-layout:0.0 -p -S -5000
cargo test yuanbao -- --nocapture
npm run smoke:gateway:yuanbao
```

## 2026-05-28 Gateway / Components 分层校正主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 `src/gateway` root owner、`src/tui/components/{ask,confirm,fork,process_bar}` 组件 owner 和本次触达组件 i18n 接入。沿用固定 lane 表，当前可用下列命令查看历史 session 或验证细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
cargo fmt --check
cargo check --all-targets
cargo test
git diff --check
```

## 2026-05-28 TUI i18n Surface Cleanup 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 Run timeline、对话输入区、compact bulletin、旧右侧面板路径和空输入 placeholder 的 i18n 收口。沿用固定 lane 表，当前可用下列命令查看历史 session 或验证细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
cargo fmt --check
cargo check --all-targets
cargo test
git diff --check
```

## 2026-05-28 TUI Menu / State i18n Cleanup 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 完成 compact sidebar、命令菜单、ASK/Confirm 菜单、计划菜单和 PlanState 标签的 i18n 收口。沿用固定 lane 表，当前可用下列命令查看历史 session 或验证细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
cargo fmt --check
cargo check --all-targets
cargo test
git diff --check
```

## 2026-05-28 TUI Main Status i18n Cleanup 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 继续清理 `src/main.rs` 中顶部状态、runtime/status、fork、selection 和 right-panel 状态文案。沿用固定 lane 表，当前可用下列命令查看历史 session 或验证细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
cargo fmt --check
cargo check --all-targets
cargo test
git diff --check
```

## 2026-05-28 TUI Render Copy i18n Cleanup 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 继续清理 `src/main.rs` 中 context row、Thought fallback、code/flowchart、TODO 状态和 ASK Other 阻塞提示文案。沿用固定 lane 表，当前可用下列命令查看历史 session 或验证细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
cargo fmt --check
cargo check --all-targets
cargo test
git diff --check
```

## 2026-05-28 TUI Context Detail i18n Cleanup 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 继续清理 `src/main.rs` 中 Context Window 估算、snapshot 回执、TODO 默认状态、context detail 和 execution detail 文案。沿用固定 lane 表，当前可用下列命令查看历史 session 或验证细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
cargo fmt --check
cargo check --all-targets
cargo test
git diff --check
```

## 2026-05-28 TUI Live Default i18n Cleanup 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 清理 composer 退出提示和 live socket 默认右侧状态文案。沿用固定 lane 表，当前可用下列命令查看历史 session 或验证细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
cargo fmt --check
cargo check --all-targets
cargo test
git diff --check
```

## 2026-05-28 TUI Status Normalization CopyKey Cleanup 主控切片

本轮未新增实现型子 Codex；由主控在主 worktree 清理 TODO 状态归一化和 execution child-id 识别里的本地化硬编码字符。沿用固定 lane 表，当前可用下列命令查看历史 session 或验证细节：

```bash
tmux list-sessions | rg '^(ff-cli|ff-core|wm-)'
cargo fmt --check
cargo check --all-targets
cargo test
git diff --check
```
