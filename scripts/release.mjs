// Builds the signed NSIS installer, writes the updater's latest.json, and
// publishes both as a GitHub release on the repo the updater endpoint points
// at. Usage: pnpm release [--notes "What changed"] [--dry-run]
// Bump "version" in src-tauri/tauri.conf.json first; the tag is v<version>.
// The signing key is ~/.tauri/veal-updater.key unless
// TAURI_SIGNING_PRIVATE_KEY is already set.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const notesAt = args.indexOf("--notes");
const notes = notesAt >= 0 ? args[notesAt + 1] ?? "" : "";

const conf = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const version = conf.version;
const endpoint = conf.plugins.updater.endpoints[0];
const repo = endpoint.match(/^https:\/\/github\.com\/([^/]+\/[^/]+)\/releases\//)?.[1];
if (!repo || repo.startsWith("OWNER/")) {
  console.error(`Set the updater endpoint in tauri.conf.json to your releases repo (now: ${endpoint}).`);
  process.exit(1);
}

const env = { ...process.env };
if (!env.TAURI_SIGNING_PRIVATE_KEY) {
  const keyPath = join(homedir(), ".tauri", "veal-updater.key");
  if (!existsSync(keyPath)) {
    console.error(`No signing key at ${keyPath}, and TAURI_SIGNING_PRIVATE_KEY is not set.`);
    process.exit(1);
  }
  env.TAURI_SIGNING_PRIVATE_KEY = readFileSync(keyPath, "utf8");
  env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ??= "";
}

const run = (cmd, argv) => execFileSync(cmd, argv, { stdio: "inherit", env, shell: process.platform === "win32" });

console.log(`Building Veal ${version}…`);
run("pnpm", ["tauri", "build"]);

const dir = "src-tauri/target/release/bundle/nsis";
const installer = `Veal_${version}_x64-setup.exe`;
const sigPath = join(dir, `${installer}.sig`);
if (!existsSync(sigPath)) {
  console.error(`${sigPath} is missing; the build did not sign the installer.`);
  process.exit(1);
}

const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(sigPath, "utf8").trim(),
      url: `https://github.com/${repo}/releases/download/v${version}/${installer}`,
    },
  },
};
const latestPath = join(dir, "latest.json");
writeFileSync(latestPath, JSON.stringify(latest, null, 2) + "\n");
console.log(`Wrote ${latestPath}`);

if (dryRun) {
  console.log("Dry run: not publishing.");
  process.exit(0);
}
run("gh", ["release", "create", `v${version}`, "--repo", repo, "--title", `Veal ${version}`, "--notes", notes || `Veal ${version}`, join(dir, installer), latestPath]);
console.log(`Published v${version} to https://github.com/${repo}/releases`);
