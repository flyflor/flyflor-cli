import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { createServer as createTcpServer, type Socket } from "node:net";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

interface Report {
  failedChecks: string[];
  gatewayLogPath: string;
  ircLines: string[];
  kernelMessages: unknown[];
  ok: boolean;
  outputDir: string;
}

const root = resolve(new URL("..", import.meta.url).pathname);
const tempRoot = mkdtempSync(join(tmpdir(), "flyflor-irc-smoke-"));
const cliHome = resolve(tempRoot, "cli-home");
const gatewayLogPath = resolve(cliHome, "logs", "gateway.log");
const channel = "#flyflor-smoke";
const outputDir = resolve(root, ".flyflor-cli", "irc-smoke", timestamp());

mkdirSync(outputDir, { recursive: true });

const kernelMessages: unknown[] = [];
const ircLines: string[] = [];
let gateway: ReturnType<typeof spawn> | undefined;
let cleanup: (() => void) | undefined;

try {
  const irc = await startIrcServer(ircLines);
  const kernel = await startKernelServer(kernelMessages);
  cleanup = () => {
    kernel.close();
    irc.close();
  };
  gateway = spawn("cargo", ["run", "--quiet", "--", "gateway", "run"], {
    cwd: root,
    env: {
      ...process.env,
      FLYFLOR_CLI_HOME: cliHome,
      FLYFLOR_GATEWAY_CHANNELS: "irc",
      FLYFLOR_GATEWAY_POLL_INTERVAL_MS: "100",
      FLYFLOR_LOG: resolve(cliHome, "logs", "channels.log"),
      FLYFLOR_WS_URL: `ws://127.0.0.1:${kernel.port}/ws`,
      IRC_ALLOWED_USERS: "ada",
      IRC_CHANNEL: channel,
      IRC_NICKNAME: "flyflorbot",
      IRC_PORT: String(irc.port),
      IRC_SERVER: "127.0.0.1",
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
          text: "irc reply",
          metadata: { smoke: true },
        },
      },
      ts: Date.now(),
    })
  );
  await waitFor(
    () => ircLines.find((line) => line === `PRIVMSG ${channel} :irc reply`),
    15_000,
    "irc outbound PRIVMSG"
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
    ircLines,
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
  if (objectField(inbound, "payload.text") !== "hello from irc") {
    failedChecks.push("gateway.message.send text mismatch");
  }
  if (objectField(inbound, "payload.conversationKey") !== `irc:${channel}`) {
    failedChecks.push("gateway.message.send conversationKey mismatch");
  }
  if (objectField(inbound, "payload.metadata.channel.platform") !== "irc") {
    failedChecks.push("gateway.message.send metadata platform mismatch");
  }
  if (!ircLines.includes("NICK flyflorbot")) failedChecks.push("irc NICK missing");
  if (!ircLines.includes(`JOIN ${channel}`)) failedChecks.push("irc JOIN missing");
  if (!ircLines.includes(`PRIVMSG ${channel} :irc reply`)) failedChecks.push("irc reply missing");
  const gatewayLog = safeRead(gatewayLogPath);
  if (!gatewayLog.includes("gateway channel runtime requested")) {
    failedChecks.push("gateway log does not show channel runtime start");
  }
  return {
    failedChecks,
    gatewayLogPath,
    ircLines,
    kernelMessages,
    ok: failedChecks.length === 0,
    outputDir,
  };
}

async function startIrcServer(ircLines: string[]) {
  let client: Socket | undefined;
  let sentInbound = false;
  const server = createTcpServer((socket) => {
    client = socket;
    let buffer = "";
    socket.on("data", (chunk) => {
      buffer += String(chunk);
      while (buffer.includes("\n")) {
        const index = buffer.indexOf("\n");
        const line = buffer.slice(0, index).replace(/\r$/, "");
        buffer = buffer.slice(index + 1);
        if (!line) continue;
        ircLines.push(line);
        if (!sentInbound && line === `JOIN ${channel}`) {
          sentInbound = true;
          socket.write(":server 001 flyflorbot :welcome\r\n");
          socket.write(`:ada!user@example PRIVMSG ${channel} :hello from irc\r\n`);
        }
      }
    });
  });
  await listenTcp(server, 0);
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("irc server did not bind a TCP port");
  return {
    close: () => {
      client?.destroy();
      server.close();
    },
    port: address.port,
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

function listenTcp(server: ReturnType<typeof createTcpServer>, port: number): Promise<void> {
  return new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(port, "127.0.0.1", () => {
      server.off("error", rejectListen);
      resolveListen();
    });
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
