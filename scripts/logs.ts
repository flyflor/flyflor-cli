import { spawn } from "node:child_process";
import { existsSync, mkdirSync, openSync, closeSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const logFile = resolve(root, ".flyflor-cli", "logs", "dev.log");

mkdirSync(dirname(logFile), { recursive: true });
if (!existsSync(logFile)) {
  closeSync(openSync(logFile, "a"));
}

const tail = spawn("tail", ["-n", "200", "-f", logFile], {
  stdio: "inherit",
});

tail.on("exit", (code) => {
  process.exit(code ?? 0);
});
