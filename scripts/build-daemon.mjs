// Builds veal-daemon.exe next to veal.exe, so the app can launch it
// (plan-19 5.11). `tauri dev` builds only the `veal` package, so without this
// no dev window could start a daemon.
//
// Usage: node scripts/build-daemon.mjs [--release] [--sidecar]
//
// `--sidecar` also copies the release exe to
// src-tauri/binaries/veal-daemon-x86_64-pc-windows-msvc.exe, where
// `bundle.externalBin` in tauri.bundle.conf.json picks it up (plan-19 7.1).
// `pnpm bundle` runs it that way; a plain `pnpm tauri build` has no daemon.
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";

const release = process.argv.includes("--release");
const sidecar = process.argv.includes("--sidecar");
if (sidecar && !release) {
  console.error("build-daemon: --sidecar needs --release");
  process.exit(1);
}
const args = ["build", "-p", "veal-daemon", "--manifest-path", "src-tauri/Cargo.toml"];
if (release) args.push("--release");

const run = spawnSync("cargo", args, { stdio: "inherit", shell: process.platform === "win32" });
if (run.error) {
  console.error(`build-daemon: ${run.error.message}`);
  process.exit(1);
}
if (run.status !== 0) process.exit(run.status ?? 1);

if (sidecar) {
  const src = "src-tauri/target/release/veal-daemon.exe";
  const dest = "src-tauri/binaries/veal-daemon-x86_64-pc-windows-msvc.exe";
  mkdirSync("src-tauri/binaries", { recursive: true });
  try {
    copyFileSync(src, dest);
  } catch (e) {
    const busy = e.code === "EBUSY" || e.code === "EPERM";
    console.error(`build-daemon: could not copy ${src} to ${dest}: ${e.message}`);
    if (busy) console.error("build-daemon: a veal-daemon.exe started from that file is still running; stop it and run again.");
    process.exit(1);
  }
  console.log(`build-daemon: ${dest}`);
}
