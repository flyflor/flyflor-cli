#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const { existsSync } = require("node:fs");
const { dirname, resolve } = require("node:path");

const root = resolve(dirname(__filename), "..");
const exeName = process.platform === "win32" ? "flyflor.exe" : "flyflor";
const platformDir = `${process.platform}-${process.arch}`;
const candidates = [
  resolve(root, "dist", platformDir, exeName),
  resolve(root, "target", "release", exeName),
  resolve(root, "target", "debug", exeName),
];

const binary = candidates.find((candidate) => existsSync(candidate));

if (!binary) {
  console.error(
    "flyflor binary is missing. Run `npm rebuild -g flyflor-cli` or reinstall the package.",
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), {
  stdio: "inherit",
  env: process.env,
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

if (result.signal) {
  process.kill(process.pid, result.signal);
}

process.exit(result.status ?? 0);
