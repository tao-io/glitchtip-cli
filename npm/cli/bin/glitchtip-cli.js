#!/usr/bin/env node
"use strict";

const { spawnSync } = require("child_process");

const platform = process.platform;
const arch = process.arch;
const pkgName = `@glitchtip/cli-${platform}-${arch}`;
const binName = platform === "win32" ? "glitchtip-cli.exe" : "glitchtip-cli";

let binPath;
try {
  binPath = require.resolve(`${pkgName}/bin/${binName}`);
} catch (_err) {
  console.error(
    `[@glitchtip/cli] No prebuilt binary available for ${platform}-${arch}.\n` +
      `Expected optional dependency \`${pkgName}\` to be installed.\n` +
      `Supported platforms: linux-x64, linux-arm64, darwin-x64, darwin-arm64, win32-x64.\n\n` +
      `If your package manager skipped optional dependencies, re-install without ` +
      `\`--no-optional\` / \`--omit=optional\`. On other platforms, build from source: ` +
      `https://gitlab.com/glitchtip/glitchtip-cli`,
  );
  process.exit(1);
}

const result = spawnSync(binPath, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(result.error);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
