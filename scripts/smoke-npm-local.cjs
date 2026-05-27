"use strict";

const { existsSync, mkdtempSync, rmSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { basename, resolve } = require("node:path");
const { spawnSync } = require("node:child_process");

const root = resolve(__dirname, "..");
const temp = mkdtempSync(resolve(tmpdir(), "flyflor-cli-npm-"));
const prefix = resolve(temp, "prefix");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: "utf8",
    stdio: options.capture ? "pipe" : "inherit",
    env: process.env,
  });

  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;
    throw new Error(`${command} ${args.join(" ")} failed\n${output}`);
  }
  return result;
}

try {
  const pack = run("npm", ["pack", "--pack-destination", temp], { capture: true });
  const tarballName = pack.stdout.trim().split(/\s+/).pop();
  if (!tarballName) {
    throw new Error("npm pack did not report a tarball");
  }

  const tarball = resolve(temp, basename(tarballName));
  run("npm", ["install", "-g", "--prefix", prefix, tarball]);

  const flyflor = process.platform === "win32"
    ? resolve(prefix, "flyflor.cmd")
    : resolve(prefix, "bin", "flyflor");
  const packageRoot = [
    resolve(prefix, "lib", "node_modules", "flyflor-cli"),
    resolve(prefix, "node_modules", "flyflor-cli"),
  ].find((candidate) => existsSync(candidate));
  if (!packageRoot) {
    throw new Error("installed flyflor-cli package root is missing");
  }
  const binary = resolve(
    packageRoot,
    "dist",
    `${process.platform}-${process.arch}`,
    process.platform === "win32" ? "flyflor.exe" : "flyflor",
  );

  if (!existsSync(flyflor)) {
    throw new Error(`installed flyflor bin is missing: ${flyflor}`);
  }
  if (!existsSync(binary)) {
    throw new Error(`installed platform binary is missing: ${binary}`);
  }

  if (process.env.FLYFLOR_NPM_SMOKE_HELP === "1") {
    const rootHelp = run(flyflor, ["-h"], { capture: true });
    const gatewayHelp = run(flyflor, ["gateway", "-h"], { capture: true });

    const rootOutput = `${rootHelp.stdout}${rootHelp.stderr}`;
    const gatewayOutput = `${gatewayHelp.stdout}${gatewayHelp.stderr}`;
    if (!rootOutput.includes("Usage:") || !rootOutput.includes("flyflor gateway [OPTIONS]")) {
      throw new Error("root help output did not include expected usage");
    }
    if (!gatewayOutput.includes("flyflor gateway") || !gatewayOutput.includes("Start gateway runtime daemon")) {
      throw new Error("gateway help output did not include expected usage");
    }
  }

  console.log("local npm pack/install smoke passed");
} finally {
  rmSync(temp, { recursive: true, force: true });
}
