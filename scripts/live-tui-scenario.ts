import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

interface LiveTuiReport {
  capturePath: string;
  cliLogPath: string;
  failedChecks: string[];
  kernelLogPath: string;
  ok: boolean;
  outputDir: string;
  sessions: {
    kernel: string;
    tui: string;
  };
  wsUrl: string;
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const kernelRoot = resolve(root, "..", "flyflor");
const outputDir = resolve(root, ".flyflor-cli", "live", timestamp());
const keepTmux = process.argv.includes("--keep-tmux");
const tempRoot = mkScenarioRoot();
const tempHome = resolve(tempRoot, "home");
const tempConfigDir = resolve(tempHome, ".config");
const tempWorkspace = resolve(tempRoot, "workspace");
const kernelSession = "flyflor-live-kernel";
const tuiSession = "flyflor-live-tui";
const kernelLogPath = resolve(outputDir, "kernel.log");
const cliLogPath = resolve(outputDir, "cli.log");
const capturePath = resolve(outputDir, "tui.capture.txt");
const reportPath = resolve(outputDir, "report.json");

mkdirSync(outputDir, { recursive: true });

try {
  killSession(kernelSession);
  killSession(tuiSession);
  prepareKernelRuntime();
  startKernel();
  const wsUrl = waitForWsUrl(kernelLogPath);
  startTui(wsUrl);
  driveTui();
  const capture = capturePane(tuiSession);
  writeFileSync(capturePath, capture);
  const report = buildReport(wsUrl, capture);
  writeFileSync(reportPath, JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
  if (!keepTmux) {
    killSession(kernelSession);
    killSession(tuiSession);
    cleanupScenarioRoot();
  }
  if (!report.ok) process.exitCode = 1;
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  const report: LiveTuiReport = {
    capturePath,
    cliLogPath,
    failedChecks: [message],
    kernelLogPath,
    ok: false,
    outputDir,
    sessions: { kernel: kernelSession, tui: tuiSession },
    wsUrl: "",
  };
  writeFileSync(reportPath, JSON.stringify(report, null, 2));
  console.error(JSON.stringify(report, null, 2));
  if (!keepTmux) {
    killSession(kernelSession);
    killSession(tuiSession);
    cleanupScenarioRoot();
  }
  process.exitCode = 1;
}

function prepareKernelRuntime(): void {
  mkdirSync(tempConfigDir, { recursive: true });
  mkdirSync(tempWorkspace, { recursive: true });
  writeFileSync(resolve(tempWorkspace, "live-note.txt"), "flyflor live TUI note\n");
  writeFileSync(resolve(tempWorkspace, "package.json"), JSON.stringify({ name: "flyflor-live-tui" }, null, 2));

  const sourceConfig = resolve(kernelRoot, ".config", "config.jsonc");
  if (!existsSync(sourceConfig)) {
    throw new Error(`missing live provider config at ${sourceConfig}`);
  }
  writeIsolatedConfig(sourceConfig, resolve(tempConfigDir, "config.jsonc"));
  execFileSync("bun", ["run", "scripts/install.templates.ts", "--target", tempConfigDir], {
    cwd: kernelRoot,
    stdio: "ignore",
  });
}

function writeIsolatedConfig(source: string, destination: string): void {
  const parsed = JSON.parse(stripJsonc(readFileSync(source, "utf8"))) as {
    gateway?: Record<string, unknown>;
  };
  parsed.gateway = {
    ...(parsed.gateway ?? {}),
    host: "127.0.0.1",
    port: 0,
    stdio: false,
  };
  writeFileSync(destination, JSON.stringify(parsed, null, 2));
}

function stripJsonc(input: string): string {
  let output = "";
  let inString = false;
  let escaped = false;
  for (let index = 0; index < input.length; index += 1) {
    const current = input[index];
    const next = input[index + 1];
    if (inString) {
      output += current;
      if (escaped) {
        escaped = false;
      } else if (current === "\\") {
        escaped = true;
      } else if (current === "\"") {
        inString = false;
      }
      continue;
    }
    if (current === "\"") {
      inString = true;
      output += current;
      continue;
    }
    if (current === "/" && next === "/") {
      while (index < input.length && input[index] !== "\n") index += 1;
      output += "\n";
      continue;
    }
    if (current === "/" && next === "*") {
      index += 2;
      while (index < input.length && !(input[index] === "*" && input[index + 1] === "/")) index += 1;
      index += 1;
      continue;
    }
    output += current;
  }
  return output.replace(/,\s*([}\]])/g, "$1");
}

function startKernel(): void {
  const command = `${[
    `FLYFLOR_HOME=${shellQuote(tempHome)}`,
    `bun --conditions=browser ${shellQuote(resolve(kernelRoot, "app.ts"))} socket`,
    `2>&1 | tee ${shellQuote(kernelLogPath)}`,
  ].join(" ")} `;
  tmux([
    "new-session",
    "-d",
    "-s",
    kernelSession,
    "-n",
    "kernel",
    `cd ${shellQuote(tempWorkspace)} && ${command}`,
  ]);
}

function startTui(wsUrl: string): void {
  const command = [
    `FLYFLOR_WS_URL=${shellQuote(wsUrl)}`,
    "FLYFLOR_DEV=1",
    `FLYFLOR_LOG=${shellQuote(cliLogPath)}`,
    "cargo run -- --dev",
  ].join(" ");
  tmux(["new-session", "-d", "-s", tuiSession, "-n", "tui", `cd ${shellQuote(root)} && ${command}`]);
  waitForText(cliLogPath, "socket connected", 45_000, "CLI socket connection");
}

function driveTui(): void {
  sendKeys("/confirm");
  sleep(500);
  sendKeys([
    "Use the Flyflor live TUI path. Reply briefly after inspecting live-note.txt if tools are available; ",
    "if a tool Confirm appears, wait for my menu confirmation.",
  ].join(""));
  sleep(18_000);
  sendKeys("/ask");
  sleep(500);
  tmux(["send-keys", "-t", tuiSession, "Enter"]);
  sleep(6_000);
  sendKeys("/history");
  sleep(2_000);
  sendKeys("/status");
  sleep(2_000);
}

function sendKeys(text: string): void {
  tmux(["send-keys", "-t", tuiSession, text, "Enter"]);
}

function waitForWsUrl(path: string): string {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (existsSync(path)) {
      const text = readFileSync(path, "utf8");
      const match = text.match(/"ws":"http:\/\/([^"]+)\/ws"/);
      if (match?.[1]) {
        return `ws://${match[1]}/ws`;
      }
      const ready = text.match(/ws:\/\/[^\s"]+\/ws/);
      if (ready?.[0]) return ready[0];
    }
    sleep(250);
  }
  throw new Error(`kernel did not publish ws url in ${path}`);
}

function waitForText(path: string, needle: string, timeoutMs: number, label: string): void {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (existsSync(path) && readFileSync(path, "utf8").includes(needle)) return;
    sleep(250);
  }
  throw new Error(`${label} did not appear in ${path}`);
}

function buildReport(wsUrl: string, capture: string): LiveTuiReport {
  const cliLog = existsSync(cliLogPath) ? readFileSync(cliLogPath, "utf8") : "";
  const kernelLog = existsSync(kernelLogPath) ? readFileSync(kernelLogPath, "utf8") : "";
  const failedChecks: string[] = [];
  if (!capture.trim()) failedChecks.push("tmux capture is empty");
  if (capture.includes("unknown")) failedChecks.push("TUI capture contains unknown");
  if (!capture.includes("ASK") && !capture.includes("flyflor")) failedChecks.push("TUI capture lacks visible Flyflor/ASK surface");
  if (cliLog.includes("panic")) failedChecks.push("CLI log contains panic");
  if (kernelLog.includes("turn.error")) failedChecks.push("kernel log contains turn.error");
  if (!kernelLog.includes("start.ready")) failedChecks.push("kernel log lacks socket start.ready");
  if (!kernelLog.includes("gateway.message.send")) failedChecks.push("kernel log lacks gateway.message.send");
  if (!kernelLog.includes("mcp.tool.call.executed")) failedChecks.push("kernel log lacks mcp.tool.call.executed");
  return {
    capturePath,
    cliLogPath,
    failedChecks,
    kernelLogPath,
    ok: failedChecks.length === 0,
    outputDir,
    sessions: { kernel: kernelSession, tui: tuiSession },
    wsUrl,
  };
}

function cleanupScenarioRoot(): void {
  rmSync(tempRoot, { recursive: true, force: true });
}

function capturePane(session: string): string {
  return execFileSync("tmux", ["capture-pane", "-t", `${session}:0.0`, "-p", "-S", "-2000"], {
    encoding: "utf8",
  });
}

function killSession(session: string): void {
  spawnSync("tmux", ["kill-session", "-t", session], { stdio: "ignore" });
}

function tmux(args: string[]): void {
  execFileSync("tmux", args, { stdio: "inherit" });
}

function sleep(ms: number): void {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function shellQuote(value: string): string {
  return `'${value.replace(/'/g, "'\\''")}'`;
}

function timestamp(): string {
  return new Date().toISOString().replace(/[:.]/g, "-");
}

function mkScenarioRoot(): string {
  return mkdtempSync(join(tmpdir(), "flyflor-live-tui-"));
}
