// Builds the viban-server crate and copies the resulting binary into
// src-tauri/binaries/ with the `-<target-triple>` suffix that Tauri's
// `externalBin` mechanism requires. Run by tauri's before-commands.
//
// Usage: node scripts/build-sidecar.mjs [--release]

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const release = process.argv.includes("--release");
const profile = release ? "release" : "debug";
const exe = process.platform === "win32" ? ".exe" : "";

function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const match = out.match(/^host:\s*(.+)$/m);
  if (!match) {
    throw new Error("could not determine host target triple from `rustc -vV`");
  }
  return match[1].trim();
}

const triple = hostTriple();

const cargoArgs = ["build", "-p", "viban-server"];
if (release) cargoArgs.push("--release");
console.log(`[build-sidecar] cargo ${cargoArgs.join(" ")}`);
execFileSync("cargo", cargoArgs, { cwd: root, stdio: "inherit" });

const from = join(root, "target", profile, `viban-server${exe}`);
const destDir = join(root, "src-tauri", "binaries");
const to = join(destDir, `viban-server-${triple}${exe}`);
mkdirSync(destDir, { recursive: true });
copyFileSync(from, to);
console.log(`[build-sidecar] ${from} -> ${to}`);
