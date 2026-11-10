import { describe, expect, it } from "vitest";
import packageJson from "../package.json?raw";
import tauriConf from "../src-tauri/tauri.conf.json?raw";
import appCargo from "../src-tauri/Cargo.toml?raw";
import coreCargo from "../src-tauri/core/Cargo.toml?raw";
import daemonCargo from "../src-tauri/daemon/Cargo.toml?raw";

// The daemon's build string comes from its own crate version, so all five
// must move together (plan-19 G3).
const cargoVersion = (toml: string) => /^version = "([^"]+)"/m.exec(toml)?.[1];

describe("version", () => {
  it("is the same in package.json, tauri.conf.json and the three crates", () => {
    const app = JSON.parse(packageJson).version as string;
    expect(app).toMatch(/^\d+\.\d+\.\d+$/);
    expect({
      tauri: JSON.parse(tauriConf).version,
      veal: cargoVersion(appCargo),
      core: cargoVersion(coreCargo),
      daemon: cargoVersion(daemonCargo),
    }).toEqual({ tauri: app, veal: app, core: app, daemon: app });
  });
});
