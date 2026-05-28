import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { createServer, type IncomingMessage } from "node:http";
import { createServer as createTcpServer, type Socket } from "node:net";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

interface Report {
  callbackBodies: unknown[];
  failedChecks: string[];
  gatewayLogPath: string;
  kernelMessages: unknown[];
  ok: boolean;
  openwebuiStatus: number;
  outputDir: string;
}

const root = resolve(new URL("..", import.meta.url).pathname);
const tempRoot = mkdtempSync(join(tmpdir(), "flyflor-openwebui-smoke-"));
const cliHome = resolve(tempRoot, "cli-home");
const gatewayLogPath = resolve(cliHome, "logs", "gateway.log");
const secret = "openwebui-smoke-secret";
const outputDir = resolve(root, ".flyflor-cli", "openwebui-smoke", timestamp());

mkdirSync(outputDir, { recursive: true });

const callbackBodies: unknown[] = [];
const kernelMessages: unknown[] = [];
let gateway: ReturnType<typeof spawn> | undefined;
let cleanup: (() => void) | undefined;

try {
  const callback = await startCallbackServer(callbackBodies);
  const kernel = await startKernelServer(kernelMessages);
  cleanup = () => {
    kernel.close();
    callback.server.close();
  };
  const openwebuiPort = await freePort();
  gateway = spawn("cargo", ["run", "--quiet", "--", "gateway", "run"], {
    cwd: root,
    env: {
      ...process.env,
      FLYFLOR_CLI_HOME: cliHome,
      FLYFLOR_GATEWAY_CHANNELS: "open-webui",
      FLYFLOR_LOG: resolve(cliHome, "logs", "channels.log"),
      FLYFLOR_WS_URL: `ws://127.0.0.1:${kernel.port}/ws`,
      OPEN_WEBUI_ALLOWED_USERS: "user-1",
      OPEN_WEBUI_BIND: `127.0.0.1:${openwebuiPort}`,
      OPEN_WEBUI_PUBLIC_URL: `http://127.0.0.1:${callback.port}/reply`,
      OPEN_WEBUI_SECRET: secret,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const stderr: string[] = [];
  gateway.stderr?.on("data", (chunk) => stderr.push(String(chunk)));
  await waitFor(() => kernel.connections.length > 0, 20_000, "gateway /ws connection");
  const openwebuiStatus = await postOpenWebui(openwebuiPort, {
    chat_id: "chat-1",
    context: { contextForkId: "fork-openwebui" },
    id: "owui-1",
    metadata: { confirmAnswer: { choiceId: "continue-tools" } },
    message: { content: "hello from open webui", id: "message-1", user_id: "user-1" },
  });
  const inbound = await waitForMessage(
    kernel.messages,
    (record) => objectField(record.value, "type") === "gateway.message.send",
    15_000,
    "gateway.message.send"
  );
  const messageId = String(
    objectField(inbound.value, "payload.messageId") ?? objectField(inbound.value, "payload.id") ?? ""
  );
  inbound.connection.sendText(
    JSON.stringify({
      id: "turn-final-smoke",
      protocol: "flyflor.ws.v1",
      type: "turn.final",
      payload: {
        reply: {
          messageId,
          text: "open webui reply",
          metadata: { smoke: true },
        },
      },
      ts: Date.now(),
    })
  );
  await waitFor(() => callbackBodies.length > 0, 15_000, "open-webui callback");

  const report = buildReport(openwebuiStatus);
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
    callbackBodies,
    failedChecks: [error instanceof Error ? error.message : String(error)],
    gatewayLogPath,
    kernelMessages,
    ok: false,
    openwebuiStatus: 0,
    outputDir,
  };
  writeFileSync(resolve(outputDir, "report.json"), JSON.stringify(report, null, 2));
  console.error(JSON.stringify(report, null, 2));
  process.exitCode = 1;
} finally {
  cleanup?.();
  if (gateway && !gateway.killed) gateway.kill("SIGTERM");
  if (!process.argv.includes("--keep-temp")) rmSync(tempRoot, { recursive: true, force: true });
}

function buildReport(openwebuiStatus: number): Report {
  const failedChecks: string[] = [];
  if (openwebuiStatus !== 202) failedChecks.push(`open-webui webhook returned ${openwebuiStatus}`);
  const inbound = kernelMessages.find((value) => objectField(value, "type") === "gateway.message.send");
  if (!inbound) failedChecks.push("mock kernel did not receive gateway.message.send");
  if (objectField(inbound, "payload.text") !== "hello from open webui") {
    failedChecks.push("gateway.message.send text mismatch");
  }
  if (objectField(inbound, "payload.conversationKey") !== "open-webui:chat-1") {
    failedChecks.push("gateway.message.send conversationKey mismatch");
  }
  if (objectField(inbound, "payload.context.contextForkId") !== "fork-openwebui") {
    failedChecks.push("gateway.message.send context missing");
  }
  if (objectField(inbound, "payload.metadata.channel.platform") !== "open-webui") {
    failedChecks.push("gateway.message.send metadata platform mismatch");
  }
  if (objectField(inbound, "payload.metadata.openWebui.confirmAnswer.choiceId") !== "continue-tools") {
    failedChecks.push("gateway.message.send metadata missing confirmAnswer");
  }
  const callback = callbackBodies[0];
  if (!callback) failedChecks.push("callback server did not receive reply");
  if (objectField(callback, "text") !== "open webui reply") failedChecks.push("callback text mismatch");
  if (objectField(callback, "route.platform") !== "open-webui") failedChecks.push("callback route mismatch");
  const gatewayLog = safeRead(gatewayLogPath);
  if (!gatewayLog.includes("gateway channel runtime requested")) {
    failedChecks.push("gateway log does not show channel runtime start");
  }
  return {
    callbackBodies,
    failedChecks,
    gatewayLogPath,
    kernelMessages,
    ok: failedChecks.length === 0,
    openwebuiStatus,
    outputDir,
  };
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

async function startCallbackServer(callbackBodies: unknown[]) {
  const server = createServer(async (request, response) => {
    const body = await readBody(request);
    try {
      callbackBodies.push(JSON.parse(body));
    } catch {
      callbackBodies.push(body);
    }
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ messageId: "callback-openwebui-1" }));
  });
  await listen(server, 0);
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("callback server did not bind a TCP port");
  return { port: address.port, server };
}

async function postOpenWebui(port: number, payload: unknown): Promise<number> {
  const response = await fetch(`http://127.0.0.1:${port}/inbound`, {
    body: JSON.stringify(payload),
    headers: {
      "content-type": "application/json",
      "x-open-webui-secret": secret,
      "x-open-webui-source": "smoke",
    },
    method: "POST",
  });
  await response.text();
  return response.status;
}

async function freePort(): Promise<number> {
  const server = createServer((_request, response) => {
    response.writeHead(404);
    response.end();
  });
  await listen(server, 0);
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("free port probe failed");
  const port = address.port;
  await new Promise<void>((resolveClose) => server.close(() => resolveClose()));
  return port;
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
  return new Promise((resolveBody, rejectBody) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
    request.on("end", () => resolveBody(Buffer.concat(chunks).toString("utf8")));
    request.on("error", rejectBody);
  });
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
