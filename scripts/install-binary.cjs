"use strict";

const { chmodSync, copyFileSync, existsSync, mkdirSync } = require("node:fs");
const { resolve } = require("node:path");
const { spawnSync } = require("node:child_process");

const root = resolve(__dirname, "..");
const exeName = process.platform === "win32" ? "flyflor.exe" : "flyflor";
const platformDir = `${process.platform}-${process.arch}`;
const outputDir = resolve(root, "dist", platformDir);
const outputBinary = resolve(outputDir, exeName);

if (existsSync(outputBinary)) {
  chmodSync(outputBinary, 0o755);
  console.log(`using bundled ${outputBinary}`);
  process.exit(0);
}

if (!existsSync(resolve(root, "Cargo.toml"))) {
  console.error(
    `flyflor has no bundled binary for ${platformDir} and no Cargo.toml fallback.`,
  );
  process.exit(1);
}

const result = spawnSync("cargo", ["build", "--release", "--bin", "flyflor"], {
  cwd: root,
  stdio: "inherit",
  env: process.env,
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

mkdirSync(outputDir, { recursive: true });
copyFileSync(resolve(root, "target", "release", exeName), outputBinary);
chmodSync(outputBinary, 0o755);
console.log(`built ${outputBinary}`);
