// Builds veal-daemon.exe next to veal.exe, so the app can launch it
// (plan-19 5.11). `tauri dev` builds only the `veal` package, so without this
// no dev window could start a daemon. Usage: node scripts/build-daemon.mjs
// [--release]
//
// Step 7 adds the copy to binaries/veal-daemon-x86_64-pc-windows-msvc.exe
// that `bundle.externalBin` needs, and runs this from beforeBuildCommand.
import { spawnSync } from "node:child_process";

const release = process.argv.includes("--release");
const args = ["build", "-p", "veal-daemon", "--manifest-path", "src-tauri/Cargo.toml"];
if (release) args.push("--release");

const run = spawnSync("cargo", args, { stdio: "inherit", shell: process.platform === "win32" });
if (run.error) {
  console.error(`build-daemon: ${run.error.message}`);
  process.exit(1);
}
process.exit(run.status ?? 1);
