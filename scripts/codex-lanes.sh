#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
SESSION_PREFIX="ff-cli"
LANES="gateway-core-runtime channels-western channels-cn channels-longtail tui-ask-layout"
LAUNCH_CODEX="0"

while [ "$#" -gt 0 ]; do
    case "$1" in
        --launch-codex)
            LAUNCH_CODEX="1"
            ;;
        --session-prefix=*)
            SESSION_PREFIX=${1#--session-prefix=}
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
    shift
done

ensure_worktree() {
    lane="$1"
    branch="feature/$lane"
    path="$ROOT_DIR/.worktrees/$lane"
    if [ ! -e "$path/.git" ]; then
        if git -C "$ROOT_DIR" show-ref --verify --quiet "refs/heads/$branch"; then
            git -C "$ROOT_DIR" worktree add "$path" "$branch" >/dev/null
        else
            git -C "$ROOT_DIR" worktree add -b "$branch" "$path" >/dev/null
        fi
    fi
    if [ -e "$ROOT_DIR/node_modules" ] && [ ! -e "$path/node_modules" ]; then
        ln -s "$ROOT_DIR/node_modules" "$path/node_modules"
    fi
    if [ -e "$ROOT_DIR/target" ] && [ ! -e "$path/target" ]; then
        ln -s "$ROOT_DIR/target" "$path/target"
    fi
}

lane_prompt() {
    lane="$1"
    case "$lane" in
        gateway-core-runtime)
            printf "%s" "Read AGENTS.md, TODO.md, LOGS.md, and session-table.md. Own only CLI gateway core runtime: config, registry, capability matrix, normalized channel bridge, doctor, unavailable/degraded semantics, and tests. Do not write kernel DBs. Keep channel identity as routing/audit metadata only. Update TODO.md/LOGS.md append-only and commit the branch."
            ;;
        channels-western)
            printf "%s" "Read AGENTS.md, TODO.md, LOGS.md, and session-table.md. Own only western messaging adapters: Telegram, Discord, Slack, Matrix, WhatsApp, Email, Webhook. Preserve ASK answers as structured metadata, not plain user text. Missing credentials/deps must be unavailable or degraded. Update TODO.md/LOGS.md append-only and commit the branch."
            ;;
        channels-cn)
            printf "%s" "Read AGENTS.md, TODO.md, LOGS.md, and session-table.md. Own only CN messaging adapters: Feishu, DingTalk, WeCom, WeCom Callback, Weixin, QQBot, Yuanbao. Keep cards/buttons as structured ASK/citizen-permission metadata. Keep QR/account state under CLI home. Update TODO.md/LOGS.md append-only and commit the branch."
            ;;
        channels-longtail)
            printf "%s" "Read AGENTS.md, TODO.md, LOGS.md, and session-table.md. Own only longtail adapters: Google Chat, IRC, ntfy, SimpleX, LINE, Mattermost, Signal, SMS, BlueBubbles, Home Assistant, Open WebUI, Teams and Graph webhook. Callback servers live in CLI gateway only. Update TODO.md/LOGS.md append-only and commit the branch."
            ;;
        tui-ask-layout)
            printf "%s" "Read AGENTS.md, TODO.md, LOGS.md, and session-table.md. Own only TUI ASK/layout refinements. Do not change ASK menu visual styling unless required for a bug. Ordinary composer input must not confirm pending ASK. Update TODO.md/LOGS.md append-only and commit the branch."
            ;;
    esac
}

for lane in $LANES; do
    ensure_worktree "$lane"
    path="$ROOT_DIR/.worktrees/$lane"
    session="$SESSION_PREFIX-$lane"
    if ! tmux has-session -t "$session" 2>/dev/null; then
        tmux new-session -d -s "$session" -c "$path"
        tmux send-keys -t "$session:0.0" "printf '%s\n' 'lane: $lane' 'worktree: $path' 'branch: feature/$lane'" C-m
    fi
    if [ "$LAUNCH_CODEX" = "1" ]; then
        prompt=$(lane_prompt "$lane")
        tmux send-keys -t "$session:0.0" "codex \"$prompt\"" C-m
    fi
done

echo "| Lane | Branch | Worktree | Attach | Capture |"
echo "|---|---|---|---|---|"
for lane in $LANES; do
    path="$ROOT_DIR/.worktrees/$lane"
    session="$SESSION_PREFIX-$lane"
    echo "| $lane | feature/$lane | $path | tmux attach -t $session | tmux capture-pane -t $session:0.0 -p -S -5000 |"
done
