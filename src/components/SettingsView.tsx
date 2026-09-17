// Settings sheet (mockup `#view-settings`). Every control writes through to
// veal.db on change; read-only rows state what the app currently does.
import { homeDir } from "@tauri-apps/api/path";
import { useEffect, useState } from "react";
import { PANE_CAP } from "../layout/tree";
import { normalizePath, tilde } from "../paths";
import { SHELL_CHOICES, useSettings, type ShellChoice } from "../settings/store";

export function SettingsView() {
  const { defaultKind, shell, restoreLayout, setDefaultKind, setShell, setRestoreLayout } = useSettings();
  const [home, setHome] = useState("~");

  useEffect(() => {
    homeDir().then((h) => setHome(tilde(normalizePath(h)))).catch(() => {});
  }, []);

  return (
    <>
      <div className="toolbar">
        <span style={{ color: "var(--text)" }}>Settings</span>
        <span className="grow" />
        <span className="hint">Saved to veal.db</span>
      </div>
      <div className="sheet">
        <div className="srow">
          <div className="lbl"><b>Default pane type</b><span>Preselected when you open a new terminal</span></div>
          <div className="seg" role="radiogroup">
            <button className={defaultKind === "shell" ? "on" : ""} onClick={() => setDefaultKind("shell")}>Shell</button>
            <button className={defaultKind === "agent" ? "on" : ""} onClick={() => setDefaultKind("agent")}>Agent</button>
          </div>
        </div>
        <div className="srow">
          <div className="lbl"><b>Shell</b><span>Launched in every new pane; open panes keep theirs</span></div>
          <select value={shell} onChange={(e) => setShell(e.target.value as ShellChoice)}>
            {SHELL_CHOICES.map((c) => (
              <option key={c.value} value={c.value}>{c.label}</option>
            ))}
          </select>
        </div>
        <div className="srow">
          <div className="lbl"><b>New projects go in</b><span>Used when you leave the root directory blank</span></div>
          <span className="mono">{home}</span>
        </div>
        <div className="srow">
          <div className="lbl"><b>Pane soft cap</b><span>New terminal is disabled past this</span></div>
          <span className="mono">{PANE_CAP}</span>
        </div>
        <div className="srow">
          <div className="lbl"><b>Task files</b><span>Per-project source of truth, git-trackable (step 6)</span></div>
          <span className="mono">.veal/tasks.json</span>
        </div>
        <div className="srow" style={{ border: 0 }}>
          <div className="lbl"><b>Restore layout on launch</b><span>Reopens panes, cwds and weights</span></div>
          <label>
            <input type="checkbox" checked={restoreLayout} onChange={(e) => setRestoreLayout(e.target.checked)} /> Enabled
          </label>
        </div>
      </div>
    </>
  );
}
