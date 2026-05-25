import { spawn, type ChildProcess } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, watch } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const logDir = resolve(root, ".flyflor-cli", "logs");
const logFile = resolve(logDir, "dev.log");
const watchTargets = [
  "src",
  "Cargo.toml",
  "Cargo.lock",
  "README.md",
].map((target) => resolve(root, target));

let child: ChildProcess | undefined;
let restartTimer: NodeJS.Timeout | undefined;
let stopping = false;

function log(message: string): void {
  mkdirSync(logDir, { recursive: true });
  appendFileSync(logFile, `${new Date().toISOString()} dev ${message}\n`);
}

function restoreTerminal(): void {
  process.stdout.write("\x1b[?25h\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?1049l");
  if (process.stdin.isTTY) {
    process.stdin.setRawMode(false);
  }
}

function start(): void {
  restoreTerminal();
  log("starting cargo run -- --dev");
  child = spawn("cargo", ["run", "--", "--dev"], {
    cwd: root,
    env: {
      ...process.env,
      FLYFLOR_DEV: "1",
      FLYFLOR_LOG: logFile,
    },
    stdio: "inherit",
  });

  child.on("exit", (code, signal) => {
    log(`child exit code=${code ?? "null"} signal=${signal ?? "null"}`);
    child = undefined;
    restoreTerminal();
    if (!stopping) {
      scheduleRestart();
    }
  });
}

function stopCurrent(onStopped: () => void): void {
  if (!child || child.killed) {
    onStopped();
    return;
  }

  const processToStop = child;
  const killTimer = setTimeout(() => {
    log("child did not exit after SIGTERM; sending SIGKILL");
    processToStop.kill("SIGKILL");
  }, 1500);

  processToStop.once("exit", () => {
    clearTimeout(killTimer);
    onStopped();
  });
  restoreTerminal();
  log("stopping child with SIGTERM");
  processToStop.kill("SIGTERM");
}

function scheduleRestart(): void {
  if (stopping) {
    return;
  }
  if (restartTimer) {
    clearTimeout(restartTimer);
  }
  restartTimer = setTimeout(() => {
    restartTimer = undefined;
    log("restarting after file change");
    stopCurrent(start);
  }, 200);
}

for (const target of watchTargets) {
  if (!existsSync(target)) {
    continue;
  }
  watch(target, { recursive: true }, (_event, filename) => {
    if (filename && ignoredPath(filename.toString())) {
      return;
    }
    scheduleRestart();
  });
}

function ignoredPath(path: string): boolean {
  return path.includes("target/") || path.endsWith(".swp") || path.endsWith("~");
}

function shutdown(signal: NodeJS.Signals): void {
  stopping = true;
  log(`shutdown ${signal}`);
  if (restartTimer) {
    clearTimeout(restartTimer);
  }
  stopCurrent(() => {
    restoreTerminal();
    process.kill(process.pid, signal);
  });
}

process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);

start();
