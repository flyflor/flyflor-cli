# Gateway Channel Closure Audit

Date: 2026-05-28

This document is an additive audit record. Existing historical TODO/LOGS entries that mention `planned` channels are append-only history, not the current catalog state.

## Current conclusion

- `flyflor-cli` currently advertises 27 gateway channels.
- All 27 catalog entries report `native_runtime=true`.
- All 27 channels have a local `*-gateway-smoke.ts` scenario script.
- The CLI gateway remains a thin `/ws` client. It does not write `brain.db` or `scope.db`.
- `planned` may still exist as a protocol/status word in old records or enum surfaces, but no active gateway catalog channel is currently planned.

## Verification

- `cargo run --quiet -- gateway config list`: 27 rows, every row `native_runtime=true`.
- `rg --files scripts | rg 'gateway-smoke\.ts$' | wc -l`: 27.
- `rg -n '"smoke:gateway:' package.json | wc -l`: 27.
- `cargo test telegram -- --nocapture`: 5 passed.
- `cargo test weixin -- --nocapture`: 3 passed.
- `npm run smoke:gateway:telegram`: ok true.
- `npm run smoke:gateway:weixin`: ok true.
- Sequential 27-smoke sweep (`dingtalk ... yuanbao`): `ALL_GATEWAY_SMOKES_OK`.
- `cargo fmt --check`: passed.
- `cargo check --all-targets`: passed.
- `cargo test gateway -- --nocapture`: 171 passed.
- `cargo test`: 363 passed.

## Channel matrix

| Channel | Adapter file | Smoke script | Current native subset | Explicit follow-up |
|---|---|---|---|---|
| Telegram | `src/gateway/channels/telegram.rs` | `scripts/telegram-gateway-smoke.ts` | Bot API `getUpdates`, text send, typing, edit-capable stream anchor. | Real credential sandbox, richer media/file delivery. |
| Discord | `src/gateway/channels/discord.rs` | `scripts/discord-gateway-smoke.ts` | REST/webhook-style message normalization and text reply. | Gateway event stream, interactions, attachments. |
| Slack | `src/gateway/channels/slack.rs` | `scripts/slack-gateway-smoke.ts` | Web API text route, allowlist, thread anchor. | Events API listener, files, richer block/card interactions. |
| Matrix | `src/gateway/channels/matrix.rs` | `scripts/matrix-gateway-smoke.ts` | Sync event normalization and text send. | Long-running sync runtime hardening, media. |
| WhatsApp | `src/gateway/channels/whatsapp.rs` | `scripts/whatsapp-gateway-smoke.ts` | Cloud API webhook payload and text send. | Verification webhook, media templates. |
| Feishu/Lark | `src/gateway/channels/feishu.rs` | `scripts/feishu-gateway-smoke.ts` | Open Platform message event and card/text reply subset. | Full card update lifecycle, file/media. |
| DingTalk | `src/gateway/channels/dingtalk.rs` | `scripts/dingtalk-gateway-smoke.ts` | OpenAPI message payload and text reply. | Callback listener and media. |
| WeCom | `src/gateway/channels/wecom.rs` | `scripts/wecom-gateway-smoke.ts` | AI Bot callback payload and WebSocket markdown reply. | Persistent listener, QR/setup, media, edit. |
| WeCom Callback | `src/gateway/channels/wecom_callback.rs` | `scripts/wecom-callback-gateway-smoke.ts` | Corp callback JSON and Corp API direct text send. | AES/XML listener, token cache, group routing. |
| Weixin iLink | `src/gateway/channels/weixin.rs` | `scripts/weixin-gateway-smoke.ts` | iLink getupdates, context token persistence, text send, typing ticket path. | Real account sandbox, media download/upload, edit. |
| QQBot | `src/gateway/channels/qqbot.rs` | `scripts/qqbot-gateway-smoke.ts` | Official API v2 payload and group/direct/guild text send. | WebSocket gateway, QR setup, markdown/keyboard/media. |
| Email | `src/gateway/channels/email.rs` | `scripts/email-gateway-smoke.ts` | Inbound email payload and SMTP text reply. | IMAP/SMTP production setup, attachments. |
| Webhook | `src/gateway/channels/webhook.rs` | `scripts/webhook-gateway-smoke.ts` | Generic HTTP payload and callback text reply. | Listener deployment hardening. |
| Microsoft Teams | `src/gateway/channels/teams.rs` | `scripts/teams-gateway-smoke.ts` | Bot Framework activity payload, incoming webhook text, Graph fallback. | Bot Framework OAuth proactive send, adaptive cards, media/edit. |
| Microsoft Graph Webhook | `src/gateway/channels/msgraph_webhook.rs` | `scripts/msgraph-webhook-gateway-smoke.ts` | Change notification payload and explicit reply webhook delivery. | ValidationToken listener, subscription lifecycle, Graph hydration. |
| Google Chat | `src/gateway/channels/google_chat.rs` | `scripts/google-chat-gateway-smoke.ts` | Pub/Sub-style payload and Chat REST text reply. | Streaming pull, OAuth/JWT mint, cards/files. |
| IRC | `src/gateway/channels/irc.rs` | `scripts/irc-gateway-smoke.ts` | TCP IRC PRIVMSG parse and text response. | TLS/SASL/runtime reconnect. |
| ntfy | `src/gateway/channels/ntfy.rs` | `scripts/ntfy-gateway-smoke.ts` | JSONL/array event polling and publish reply. | Auth/topic production hardening. |
| SimpleX | `src/gateway/channels/simplex.rs` | `scripts/simplex-gateway-smoke.ts` | `newChatItem` payload and daemon WebSocket text commands. | Persistent listener, file/media, setup wizard. |
| LINE | `src/gateway/channels/line.rs` | `scripts/line-gateway-smoke.ts` | Webhook event and Messaging API text reply. | Signature listener, rich messages/media. |
| Mattermost | `src/gateway/channels/mattermost.rs` | `scripts/mattermost-gateway-smoke.ts` | REST posts polling and text reply. | WebSocket events, file/media, reactions. |
| Signal | `src/gateway/channels/signal.rs` | `scripts/signal-gateway-smoke.ts` | signal-cli REST envelope and JSON-RPC text send. | SSE stream, attachments, reactions, rate scheduling. |
| SMS | `src/gateway/channels/sms.rs` | `scripts/sms-gateway-smoke.ts` | Twilio-style webhook and SMS text reply. | Provider abstraction and MMS/media. |
| BlueBubbles/iMessage | `src/gateway/channels/bluebubbles.rs` | `scripts/bluebubbles-gateway-smoke.ts` | BlueBubbles webhook and text send. | Attachments, typing, richer iMessage metadata. |
| Home Assistant | `src/gateway/channels/homeassistant.rs` | `scripts/homeassistant-gateway-smoke.ts` | Webhook/event payload and service call text response. | Entity-specific automations and auth hardening. |
| Open WebUI | `src/gateway/channels/openwebui.rs` | `scripts/openwebui-gateway-smoke.ts` | Webhook payload and callback response. | Native plugin/runtime integration. |
| Yuanbao | `src/gateway/channels/yuanbao.rs` | `scripts/yuanbao-gateway-smoke.ts` | JSON push bridge and explicit reply webhook delivery. | Full HMAC/protobuf WS, heartbeat, COS media, recall patching. |

## Drift notes

- Historical append-only records can still say a channel was `planned` at that time. Those records are not edited.
- The current source guard is `channel_list_has_no_planned_channels_after_native_gateway_closure`.
- The current platform registry guard is `registry_advertises_native_platforms_without_fake_success`.
- The current smoke guard is operational: every catalog channel has one local mock gateway smoke script.
- `UnsupportedPlatformAdapter` remains as a defensive fallback for future catalog additions, but the current registry tests prove it is not used by any of the 27 active catalog channels.
