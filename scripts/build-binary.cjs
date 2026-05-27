"use strict";

const { chmodSync, copyFileSync, mkdirSync } = require("node:fs");
const { resolve } = require("node:path");
const { spawnSync } = require("node:child_process");

const root = resolve(__dirname, "..");

const targetMatrix = [
  ["aarch64-apple-darwin", "darwin-arm64", "flyflor"],
  ["x86_64-apple-darwin", "darwin-x64", "flyflor"],
  ["aarch64-unknown-linux-gnu", "linux-arm64", "flyflor"],
  ["x86_64-unknown-linux-gnu", "linux-x64", "flyflor"],
  ["aarch64-pc-windows-msvc", "win32-arm64", "flyflor.exe"],
  ["x86_64-pc-windows-msvc", "win32-x64", "flyflor.exe"],
].map(([rustTarget, platformDir, exeName]) => ({
  rustTarget,
  platformDir,
  exeName,
}));

function hostPlatformDir() {
  return `${process.platform}-${process.arch}`;
}

function hostTarget() {
  const platformDir = hostPlatformDir();
  return targetMatrix.find((target) => target.platformDir === platformDir);
}

function parseTargets(argv) {
  const requested = [];
  let all = false;

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--all") {
      all = true;
      continue;
    }
    if (arg === "--target") {
      const value = argv[index + 1];
      if (!value) throw new Error("--target requires a Rust target triple");
      requested.push(value);
      index += 1;
      continue;
    }
    if (arg.startsWith("--target=")) {
      requested.push(arg.slice("--target=".length));
      continue;
    }
    throw new Error(`unknown build-binary option: ${arg}`);
  }

  if (process.env.FLYFLOR_NPM_RUST_TARGETS) {
    requested.push(
      ...process.env.FLYFLOR_NPM_RUST_TARGETS.split(",")
        .map((target) => target.trim())
        .filter(Boolean),
    );
  }

  if (all) return targetMatrix.map((target) => ({ ...target, explicit: true }));

  if (requested.length > 0) {
    return requested.map((rustTarget) => {
      const target = targetMatrix.find((entry) => entry.rustTarget === rustTarget);
      if (!target) throw new Error(`unsupported Rust target for npm packaging: ${rustTarget}`);
      return { ...target, explicit: true };
    });
  }

  const target = hostTarget();
  if (!target) throw new Error(`unsupported npm platform: ${hostPlatformDir()}`);
  return [{ ...target, explicit: false }];
}

function buildTarget(target) {
  const cargoArgs = ["build", "--release", "--bin", "flyflor"];
  if (target.explicit) cargoArgs.push("--target", target.rustTarget);

  const result = spawnSync("cargo", cargoArgs, {
    cwd: root,
    stdio: "inherit",
    env: process.env,
  });

  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }

  const targetBinary = target.explicit
    ? resolve(root, "target", target.rustTarget, "release", target.exeName)
    : resolve(root, "target", "release", target.exeName);
  const outputDir = resolve(root, "dist", target.platformDir);
  const outputBinary = resolve(outputDir, target.exeName);

  mkdirSync(outputDir, { recursive: true });
  copyFileSync(targetBinary, outputBinary);
  chmodSync(outputBinary, 0o755);
  console.log(`built ${target.rustTarget} -> ${outputBinary}`);
}

try {
  for (const target of parseTargets(process.argv.slice(2))) {
    buildTarget(target);
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
