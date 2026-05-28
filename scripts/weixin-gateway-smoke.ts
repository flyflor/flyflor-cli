import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { createServer, type IncomingMessage } from "node:http";
import { createServer as createTcpServer } from "node:net";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

interface Report {
  failedChecks: string[];
  gatewayLogPath: string;
  kernelMessages: unknown[];
  ok: boolean;
  outputDir: string;
  weixinRequests: RequestRecord[];
}

interface RequestRecord {
  body: string;
  headers: Record<string, string[] | string | undefined>;
  method: string;
  url: string;
}

const root = resolve(new URL("..", import.meta.url).pathname);
const tempRoot = mkdtempSync(join(tmpdir(), "flyflor-weixin-smoke-"));
const cliHome = resolve(tempRoot, "cli-home");
const channelHome = resolve(cliHome, "gateway");
const gatewayLogPath = resolve(cliHome, "logs", "gateway.log");
const accountId = "bot-1";
const token = "weixin-token";
const userId = "user-1";
const messageId = "wx-msg-smoke";
const contextToken = "ctx-smoke";
const outputDir = resolve(root, ".flyflor-cli", "weixin-smoke", timestamp());

mkdirSync(outputDir, { recursive: true });

const kernelMessages: unknown[] = [];
const weixinRequests: RequestRecord[] = [];
let gateway: ReturnType<typeof spawn> | undefined;
let cleanup: (() => void) | undefined;

try {
  const weixin = await startWeixinIlinkServer(weixinRequests);
  const kernel = await startKernelServer(kernelMessages);
  cleanup = () => {
    kernel.close();
    weixin.server.close();
  };
  gateway = spawn("cargo", ["run", "--quiet", "--", "gateway", "run"], {
    cwd: root,
    env: {
      ...process.env,
      FLYFLOR_CHANNEL_HOME: channelHome,
      FLYFLOR_CLI_HOME: cliHome,
      FLYFLOR_GATEWAY_CHANNELS: "weixin",
      FLYFLOR_GATEWAY_POLL_INTERVAL_MS: "250",
      FLYFLOR_LOG: resolve(cliHome, "logs", "channels.log"),
      FLYFLOR_WS_URL: `ws://127.0.0.1:${kernel.port}/ws`,
      WEIXIN_ACCOUNT_ID: accountId,
      WEIXIN_ALLOWED_USERS: userId,
      WEIXIN_BASE_URL: `http://127.0.0.1:${weixin.port}`,
      WEIXIN_DM_POLICY: "allowlist",
      WEIXIN_SEND_CHUNK_RETRIES: "0",
      WEIXIN_TOKEN: token,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const stderr: string[] = [];
  gateway.stderr?.on("data", (chunk) => stderr.push(String(chunk)));
  await waitFor(() => kernel.connections.length > 0, 20_000, "gateway /ws connection");
  const inbound = await waitForMessage(
    kernel.messages,
    (record) => objectField(record.value, "type") === "gateway.message.send",
    20_000,
    "gateway.message.send"
  );
  const turnMessageId = String(
    objectField(inbound.value, "payload.messageId") ?? objectField(inbound.value, "payload.id") ?? ""
  );
  inbound.connection.sendText(
    JSON.stringify({
      id: "turn-final-smoke",
      payload: {
        reply: {
          messageId: turnMessageId,
          metadata: { smoke: true },
          text: "weixin reply",
        },
      },
      protocol: "flyflor.ws.v1",
      ts: Date.now(),
      type: "turn.final",
    })
  );
  await waitFor(
    () => weixinRequests.find((request) => request.method === "POST" && request.url === "/ilink/bot/sendmessage"),
    15_000,
    "Weixin sendmessage POST"
  );

  const report = buildReport();
  writeFileSync(resolve(outputDir, "report.json"), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) process.exitCode = 1;
  if (stderr.length && report.ok) {
    writeFileSync(resolve(outputDir, "gateway.stderr.log"), stderr.join(""));
  }
  cleanup();
  cleanup = undefined;
} catch (error) {
  const report: Report = {
    failedChecks: [error instanceof Error ? error.message : String(error)],
    gatewayLogPath,
    kernelMessages,
    ok: false,
    outputDir,
    weixinRequests,
  };
  writeFileSync(resolve(outputDir, "report.json"), JSON.stringify(report, null, 2));
  console.error(JSON.stringify(report, null, 2));
  process.exitCode = 1;
} finally {
  cleanup?.();
  if (gateway && !gateway.killed) gateway.kill("SIGTERM");
  if (!process.argv.includes("--keep-temp")) rmSync(tempRoot, { recursive: true, force: true });
}

function buildReport(): Report {
  const failedChecks: string[] = [];
  const inbound = kernelMessages.find((value) => objectField(value, "type") === "gateway.message.send");
  if (!inbound) failedChecks.push("mock kernel did not receive gateway.message.send");
  if (objectField(inbound, "payload.text") !== "hello from weixin") {
    failedChecks.push("gateway.message.send text mismatch");
  }
  if (objectField(inbound, "payload.conversationKey") !== `weixin:${userId}`) {
    failedChecks.push("gateway.message.send conversationKey mismatch");
  }
  if (objectField(inbound, "payload.metadata.channel.platform") !== "weixin") {
    failedChecks.push("gateway.message.send metadata platform mismatch");
  }
  if (objectField(inbound, "payload.metadata.channel.sourceMessageId") !== messageId) {
    failedChecks.push("gateway.message.send source message mismatch");
  }
  if (objectField(inbound, "payload.metadata.channel.contextTokenPresent") !== true) {
    failedChecks.push("gateway.message.send context token flag mismatch");
  }
  const post = weixinRequests.find((request) => request.method === "POST" && request.url === "/ilink/bot/sendmessage");
  if (!post) failedChecks.push("mock Weixin iLink did not receive sendmessage");
  const body = parseJson(post?.body ?? "");
  if (objectField(body, "msg.to_user_id") !== userId) {
    failedChecks.push("Weixin sendmessage to_user_id mismatch");
  }
  if (objectField(body, "msg.context_token") !== contextToken) {
    failedChecks.push("Weixin sendmessage context token mismatch");
  }
  const items = objectField(body, "msg.item_list");
  const firstItem = Array.isArray(items) ? items[0] : undefined;
  if (objectField(firstItem, "text_item.text") !== "weixin reply") {
    failedChecks.push("Weixin sendmessage text mismatch");
  }
  const gatewayLog = safeRead(gatewayLogPath);
  if (!gatewayLog.includes("gateway channel runtime requested")) {
    failedChecks.push("gateway log does not show channel runtime start");
  }
  return {
    failedChecks,
    gatewayLogPath,
    kernelMessages,
    ok: failedChecks.length === 0,
    outputDir,
    weixinRequests,
  };
}

async function startWeixinIlinkServer(records: RequestRecord[]) {
  let delivered = false;
  const server = createServer(async (request, response) => {
    const body = await readBody(request);
    records.push({
      body,
      headers: request.headers,
      method: request.method ?? "GET",
      url: request.url ?? "",
    });
    if (request.method === "POST" && request.url === "/ilink/bot/getupdates") {
      const msgs = delivered
        ? []
        : [
            {
              from_user_id: userId,
              to_user_id: accountId,
              message_id: messageId,
              context_token: contextToken,
              item_list: [{ type: 1, text_item: { text: "hello from weixin" } }],
            },
          ];
      delivered = true;
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ ret: 0, errmsg: "ok", get_updates_buf: "sync-1", msgs }));
      return;
    }
    if (request.method === "POST" && request.url === "/ilink/bot/sendmessage") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ ret: 0, errmsg: "ok", msgid: "wx-reply-message" }));
      return;
    }
    if (request.method === "POST" && request.url === "/ilink/bot/getconfig") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ ret: 0, errmsg: "ok", typing_ticket: "typing-1" }));
      return;
    }
    if (request.method === "POST" && request.url === "/ilink/bot/sendtyping") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ ret: 0, errmsg: "ok" }));
      return;
    }
    response.writeHead(404, { "content-type": "application/json" });
    response.end(JSON.stringify({ ret: 404, errmsg: "not found" }));
  });
  await listen(server, 0);
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Weixin iLink server did not bind a TCP port");
  return { port: address.port, server };
}

interface KernelMessageRecord {
  connection: KernelConnection;
  value: unknown;
}

interface KernelConnection {
  close: () => void;
  sendText: (text: string) => void;
}

async function startKernelServer(kernelMessages: unknown[]) {
  const connections: KernelConnection[] = [];
  const messages: KernelMessageRecord[] = [];
  const server = createTcpServer((socket) => {
    let buffer = Buffer.alloc(0);
    let upgraded = false;
    const connection: KernelConnection = {
      close: () => socket.destroy(),
      sendText: (text: string) => socket.write(encodeWebSocketText(text)),
    };
    connections.push(connection);
    socket.on("data", (chunk) => {
      buffer = Buffer.concat([buffer, chunk]);
      if (!upgraded) {
        const headerEnd = buffer.indexOf("\r\n\r\n");
        if (headerEnd < 0) return;
        const header = buffer.subarray(0, headerEnd).toString("utf8");
        socket.write(webSocketHandshakeResponse(header));
        buffer = buffer.subarray(headerEnd + 4);
        upgraded = true;
      }
      while (true) {
        const decoded = decodeWebSocketText(buffer);
        if (!decoded) break;
        buffer = buffer.subarray(decoded.bytes);
        let value: unknown;
        try {
          value = JSON.parse(decoded.text);
        } catch {
          value = decoded.text;
        }
        kernelMessages.push(value);
        messages.push({ connection, value });
      }
    });
  });
  await listenTcp(server, 0);
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("kernel server did not bind a TCP port");
  return {
    close: () => {
      for (const connection of connections) connection.close();
      server.close();
    },
    connections,
    messages,
    port: address.port,
  };
}

function objectField(value: unknown, path: string): unknown {
  let current: unknown = value;
  for (const part of path.split(".")) {
    if (!current || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

function waitForMessage<T>(
  values: T[],
  predicate: (value: T) => boolean,
  timeoutMs: number,
  label: string
): Promise<T> {
  return waitFor(() => values.find(predicate), timeoutMs, label);
}

async function waitFor<T>(probe: () => T | undefined | false, timeoutMs: number, label: string): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = probe();
    if (value) return value;
    await sleep(100);
  }
  throw new Error(`timed out waiting for ${label}`);
}

function listen(server: ReturnType<typeof createServer>, port: number): Promise<void> {
  return new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(port, "127.0.0.1", () => {
      server.off("error", rejectListen);
      resolveListen();
    });
  });
}

function listenTcp(server: ReturnType<typeof createTcpServer>, port: number): Promise<void> {
  return new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(port, "127.0.0.1", () => {
      server.off("error", rejectListen);
      resolveListen();
    });
  });
}

function readBody(request: IncomingMessage): Promise<string> {
  return new Promise((resolveRead, rejectRead) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
    request.on("error", rejectRead);
    request.on("end", () => resolveRead(Buffer.concat(chunks).toString("utf8")));
  });
}

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

function safeRead(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, ms));
}

function timestamp(): string {
  return new Date().toISOString().replace(/[:.]/g, "-");
}

function webSocketHandshakeResponse(header: string): string {
  const key = header
    .split(/\r?\n/)
    .map((line) => line.split(":"))
    .find(([name]) => name?.trim().toLowerCase() === "sec-websocket-key")?.[1]
    ?.trim();
  if (!key) throw new Error("websocket handshake omitted Sec-WebSocket-Key");
  const accept = createHash("sha1")
    .update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
    .digest("base64");
  return [
    "HTTP/1.1 101 Switching Protocols",
    "Upgrade: websocket",
    "Connection: Upgrade",
    `Sec-WebSocket-Accept: ${accept}`,
    "\r\n",
  ].join("\r\n");
}

function decodeWebSocketText(buffer: Buffer): { bytes: number; text: string } | undefined {
  if (buffer.length < 2) return undefined;
  const opcode = buffer[0] & 0x0f;
  if (opcode === 0x8) return { bytes: buffer.length, text: "" };
  if (opcode !== 0x1) throw new Error(`unsupported websocket opcode ${opcode}`);
  const masked = (buffer[1] & 0x80) !== 0;
  let length = buffer[1] & 0x7f;
  let offset = 2;
  if (length === 126) {
    if (buffer.length < offset + 2) return undefined;
    length = buffer.readUInt16BE(offset);
    offset += 2;
  } else if (length === 127) {
    if (buffer.length < offset + 8) return undefined;
    const big = buffer.readBigUInt64BE(offset);
    if (big > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error("websocket frame too large");
    length = Number(big);
    offset += 8;
  }
  const maskOffset = offset;
  if (masked) offset += 4;
  if (buffer.length < offset + length) return undefined;
  const payload = Buffer.from(buffer.subarray(offset, offset + length));
  if (masked) {
    const mask = buffer.subarray(maskOffset, maskOffset + 4);
    for (let index = 0; index < payload.length; index += 1) {
      payload[index] ^= mask[index % 4];
    }
  }
  return { bytes: offset + length, text: payload.toString("utf8") };
}

function encodeWebSocketText(text: string): Buffer {
  const payload = Buffer.from(text, "utf8");
  if (payload.length < 126) {
    return Buffer.concat([Buffer.from([0x81, payload.length]), payload]);
  }
  if (payload.length <= 0xffff) {
    const header = Buffer.alloc(4);
    header[0] = 0x81;
    header[1] = 126;
    header.writeUInt16BE(payload.length, 2);
    return Buffer.concat([header, payload]);
  }
  const header = Buffer.alloc(10);
  header[0] = 0x81;
  header[1] = 127;
  header.writeBigUInt64BE(BigInt(payload.length), 2);
  return Buffer.concat([header, payload]);
}
