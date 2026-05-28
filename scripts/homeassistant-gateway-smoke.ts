import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { createServer, request as httpRequest, type IncomingMessage } from "node:http";
import { createServer as createTcpServer } from "node:net";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

interface Report {
  failedChecks: string[];
  gatewayLogPath: string;
  homeassistantRequests: HomeAssistantRequestRecord[];
  kernelMessages: unknown[];
  ok: boolean;
  outputDir: string;
}

interface HomeAssistantRequestRecord {
  body: string;
  headers: Record<string, string | string[] | undefined>;
  method: string;
  url: string;
}

const root = resolve(new URL("..", import.meta.url).pathname);
const tempRoot = mkdtempSync(join(tmpdir(), "flyflor-homeassistant-smoke-"));
const cliHome = resolve(tempRoot, "cli-home");
const gatewayLogPath = resolve(cliHome, "logs", "gateway.log");
const webhookSecret = "ha-secret";
const outputDir = resolve(root, ".flyflor-cli", "homeassistant-smoke", timestamp());

mkdirSync(outputDir, { recursive: true });

const homeassistantRequests: HomeAssistantRequestRecord[] = [];
const kernelMessages: unknown[] = [];
let gateway: ReturnType<typeof spawn> | undefined;
let cleanup: (() => void) | undefined;

try {
  const homeassistant = await startHomeAssistantServer(homeassistantRequests);
  const webhook = await reservePort();
  const kernel = await startKernelServer(kernelMessages);
  cleanup = () => {
    kernel.close();
    homeassistant.server.close();
  };
  gateway = spawn("cargo", ["run", "--quiet", "--", "gateway", "run"], {
    cwd: root,
    env: {
      ...process.env,
      FLYFLOR_CLI_HOME: cliHome,
      FLYFLOR_GATEWAY_CHANNELS: "homeassistant",
      FLYFLOR_GATEWAY_POLL_INTERVAL_MS: "250",
      FLYFLOR_LOG: resolve(cliHome, "logs", "channels.log"),
      FLYFLOR_WS_URL: `ws://127.0.0.1:${kernel.port}/ws`,
      HOME_ASSISTANT_ALLOWED_USERS: "person.alice",
      HOME_ASSISTANT_TOKEN: "ha-token",
      HOME_ASSISTANT_URL: `http://127.0.0.1:${homeassistant.port}`,
      HOME_ASSISTANT_WEBHOOK_BIND: `127.0.0.1:${webhook.port}`,
      HOME_ASSISTANT_WEBHOOK_SECRET: webhookSecret,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const stderr: string[] = [];
  gateway.stderr?.on("data", (chunk) => stderr.push(String(chunk)));
  await waitFor(() => kernel.connections.length > 0, 20_000, "gateway /ws connection");
  await postJson(`http://127.0.0.1:${webhook.port}/webhook/homeassistant`, {
    conversation_id: "ha-convo-1",
    id: "ha-event-1",
    text: "hello from home assistant",
    user_id: "person.alice",
  });
  const inbound = await waitForMessage(
    kernel.messages,
    (record) => objectField(record.value, "type") === "gateway.message.send",
    20_000,
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
          text: "homeassistant reply",
          metadata: { smoke: true },
        },
      },
      ts: Date.now(),
    })
  );
  await waitFor(
    () =>
      homeassistantRequests.find(
        (request) => request.method === "POST" && request.url === "/api/conversation/process"
      ),
    15_000,
    "homeassistant conversation process"
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
    homeassistantRequests,
    kernelMessages,
    ok: false,
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

function buildReport(): Report {
  const failedChecks: string[] = [];
  const inbound = kernelMessages.find((value) => objectField(value, "type") === "gateway.message.send");
  if (!inbound) failedChecks.push("mock kernel did not receive gateway.message.send");
  if (objectField(inbound, "payload.text") !== "hello from home assistant") {
    failedChecks.push("gateway.message.send text mismatch");
  }
  if (objectField(inbound, "payload.conversationKey") !== "homeassistant:ha-convo-1") {
    failedChecks.push("gateway.message.send conversationKey mismatch");
  }
  if (objectField(inbound, "payload.metadata.channel.platform") !== "homeassistant") {
    failedChecks.push("gateway.message.send metadata platform mismatch");
  }
  const process = homeassistantRequests.find(
    (request) => request.method === "POST" && request.url === "/api/conversation/process"
  );
  if (!process) failedChecks.push("mock homeassistant did not receive conversation/process POST");
  if (process && objectField(parseJson(process.body), "conversation_id") !== "ha-convo-1") {
    failedChecks.push("homeassistant conversation id mismatch");
  }
  if (process && objectField(parseJson(process.body), "text") !== "homeassistant reply") {
    failedChecks.push("homeassistant reply text mismatch");
  }
  const gatewayLog = safeRead(gatewayLogPath);
  if (!gatewayLog.includes("gateway channel runtime requested")) {
    failedChecks.push("gateway log does not show channel runtime start");
  }
  return {
    failedChecks,
    gatewayLogPath,
    homeassistantRequests,
    kernelMessages,
    ok: failedChecks.length === 0,
    outputDir,
  };
}

async function startHomeAssistantServer(homeassistantRequests: HomeAssistantRequestRecord[]) {
  const server = createServer(async (request, response) => {
    const body = await readBody(request);
    homeassistantRequests.push({
      body,
      headers: request.headers,
      method: request.method ?? "GET",
      url: request.url ?? "",
    });
    if (request.method === "POST" && request.url === "/api/conversation/process") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(
        JSON.stringify({
          conversation_id: "ha-convo-1",
          response: { speech: { plain: { speech: "accepted" } } },
        })
      );
      return;
    }
    response.writeHead(404, { "content-type": "application/json" });
    response.end(JSON.stringify({ status_code: 404, message: "not found" }));
  });
  await listen(server, 0);
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("homeassistant server did not bind a TCP port");
  }
  return { port: address.port, server };
}

async function postJson(url: string, body: unknown): Promise<void> {
  const payload = JSON.stringify(body);
  await new Promise<void>((resolvePost, rejectPost) => {
    const request = httpRequest(
      url,
      {
        headers: {
          "authorization": `Bearer ${webhookSecret}`,
          "content-length": Buffer.byteLength(payload),
          "content-type": "application/json",
          "x-homeassistant-source": "smoke",
        },
        method: "POST",
      },
      (response) => {
        response.resume();
        response.on("end", () => {
          if ((response.statusCode ?? 500) >= 400) {
            rejectPost(new Error(`homeassistant webhook returned ${response.statusCode}`));
          } else {
            resolvePost();
          }
        });
      }
    );
    request.on("error", rejectPost);
    request.end(payload);
  });
}

async function reservePort(): Promise<{ port: number }> {
  const server = createTcpServer();
  await listenTcp(server, 0);
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("port reservation failed");
  const port = address.port;
  await new Promise<void>((resolveClose) => server.close(() => resolveClose()));
  return { port };
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
  return new Promise((resolveBody, rejectBody) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
    request.on("end", () => resolveBody(Buffer.concat(chunks).toString("utf8")));
    request.on("error", rejectBody);
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
