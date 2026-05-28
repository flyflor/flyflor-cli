import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { createServer as createTcpServer, type Socket } from "node:net";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

interface Report {
  failedChecks: string[];
  gatewayLogPath: string;
  kernelMessages: unknown[];
  ok: boolean;
  outputDir: string;
  smtpSessions: SmtpSessionRecord[];
}

interface SmtpSessionRecord {
  commands: string[];
  data: string;
}

const root = resolve(new URL("..", import.meta.url).pathname);
const tempRoot = mkdtempSync(join(tmpdir(), "flyflor-email-smoke-"));
const cliHome = resolve(tempRoot, "cli-home");
const gatewayLogPath = resolve(cliHome, "logs", "gateway.log");
const botAddress = "bot@example.com";
const userAddress = "user@example.com";
const outputDir = resolve(root, ".flyflor-cli", "email-smoke", timestamp());

mkdirSync(outputDir, { recursive: true });

const kernelMessages: unknown[] = [];
const smtpSessions: SmtpSessionRecord[] = [];
let gateway: ReturnType<typeof spawn> | undefined;
let cleanup: (() => void) | undefined;

try {
  const smtp = await startSmtpServer(smtpSessions);
  const kernel = await startKernelServer(kernelMessages);
  cleanup = () => {
    kernel.close();
    smtp.server.close();
  };
  gateway = spawn("cargo", ["run", "--quiet", "--", "gateway", "run"], {
    cwd: root,
    env: {
      ...process.env,
      EMAIL_ADDRESS: botAddress,
      EMAIL_ALLOWED_USERS: userAddress,
      EMAIL_INBOUND_MESSAGE: JSON.stringify({
        context: { contextForkId: "fork-email" },
        from: userAddress,
        id: "email-message-1",
        subject: "Hello Flyflor",
        text: "hello from email",
        to: botAddress,
      }),
      EMAIL_PASSWORD: "smtp-password",
      EMAIL_SMTP_HOST: "127.0.0.1",
      EMAIL_SMTP_PORT: String(smtp.port),
      FLYFLOR_CLI_HOME: cliHome,
      FLYFLOR_GATEWAY_CHANNELS: "email",
      FLYFLOR_GATEWAY_POLL_INTERVAL_MS: "250",
      FLYFLOR_LOG: resolve(cliHome, "logs", "channels.log"),
      FLYFLOR_WS_URL: `ws://127.0.0.1:${kernel.port}/ws`,
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
          text: "email reply",
          metadata: { smoke: true },
        },
      },
      ts: Date.now(),
    })
  );
  await waitFor(() => smtpSessions.find((session) => session.data.includes("email reply")), 15_000, "SMTP DATA");

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
    smtpSessions,
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
  if (objectField(inbound, "payload.text") !== "hello from email") {
    failedChecks.push("gateway.message.send text mismatch");
  }
  if (objectField(inbound, "payload.conversationKey") !== `email:${userAddress}`) {
    failedChecks.push("gateway.message.send conversationKey mismatch");
  }
  if (objectField(inbound, "payload.context.contextForkId") !== "fork-email") {
    failedChecks.push("gateway.message.send context missing");
  }
  if (objectField(inbound, "payload.metadata.channel.platform") !== "email") {
    failedChecks.push("gateway.message.send metadata platform mismatch");
  }
  if (objectField(inbound, "payload.metadata.channel.subject") !== "Hello Flyflor") {
    failedChecks.push("gateway.message.send subject metadata mismatch");
  }
  const session = smtpSessions.find((value) => value.data.includes("email reply"));
  if (!session) failedChecks.push("mock SMTP server did not receive DATA");
  if (!session?.commands.includes(`MAIL FROM:<${botAddress}>`)) failedChecks.push("SMTP MAIL FROM mismatch");
  if (!session?.commands.includes(`RCPT TO:<${userAddress}>`)) failedChecks.push("SMTP RCPT TO mismatch");
  if (!session?.data.includes("Subject: Re: Hello Flyflor")) failedChecks.push("SMTP subject mismatch");
  if (!session?.data.includes("In-Reply-To: <email-email-message-1>")) failedChecks.push("SMTP In-Reply-To mismatch");
  if (!session?.data.includes("\r\nemail reply\r\n.\r\n")) failedChecks.push("SMTP body mismatch");
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
    smtpSessions,
  };
}

async function startSmtpServer(smtpSessions: SmtpSessionRecord[]) {
  const server = createTcpServer((socket) => {
    const session: SmtpSessionRecord = { commands: [], data: "" };
    smtpSessions.push(session);
    let buffer = "";
    let dataMode = false;
    socket.write("220 flyflor mock smtp\r\n");
    socket.on("data", (chunk) => {
      buffer += chunk.toString("utf8");
      while (buffer.includes("\n")) {
        const lineEnd = buffer.indexOf("\n");
        const rawLine = buffer.slice(0, lineEnd + 1);
        buffer = buffer.slice(lineEnd + 1);
        const line = rawLine.replace(/\r?\n$/, "");
        if (dataMode) {
          session.data += rawLine;
          if (line === ".") {
            dataMode = false;
            socket.write("250 queued\r\n");
          }
          continue;
        }
        session.commands.push(line);
        const upper = line.toUpperCase();
        if (upper.startsWith("EHLO")) socket.write("250-localhost\r\n250 AUTH LOGIN\r\n");
        else if (upper === "AUTH LOGIN") socket.write("334 VXNlcm5hbWU6\r\n");
        else if (line === Buffer.from(botAddress).toString("base64")) socket.write("334 UGFzc3dvcmQ6\r\n");
        else if (line === Buffer.from("smtp-password").toString("base64")) socket.write("235 authenticated\r\n");
        else if (upper.startsWith("MAIL FROM:")) socket.write("250 ok\r\n");
        else if (upper.startsWith("RCPT TO:")) socket.write("250 ok\r\n");
        else if (upper === "DATA") {
          dataMode = true;
          socket.write("354 end with dot\r\n");
        } else if (upper === "QUIT") {
          socket.write("221 bye\r\n");
          socket.end();
        } else {
          socket.write("250 ok\r\n");
        }
      }
    });
  });
  await listenTcp(server, 0);
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("SMTP server did not bind a TCP port");
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
