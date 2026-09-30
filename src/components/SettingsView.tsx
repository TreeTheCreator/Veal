// Settings sheet (mockup `#view-settings`). Every control writes through to
// veal.db on change; read-only rows state what the app currently does.
import { homeDir } from "@tauri-apps/api/path";
import { useEffect, useState } from "react";
import { PANE_CAP } from "../layout/tree";
import { normalizePath, tilde } from "../paths";
import { AGENTS } from "../pty";
import { SHELL_CHOICES, useSettings, type ShellChoice } from "../settings/store";

export function SettingsView() {
  const { theme, shell, restoreLayout, setTheme, setShell, setRestoreLayout } = useSettings();
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
          <div className="lbl"><b>Appearance</b><span>Follows the OS unless set here</span></div>
          <div className="seg" role="radiogroup">
            {(["system", "dark", "light"] as const).map((t) => (
              <button key={t} className={theme === t ? "on" : ""} onClick={() => setTheme(t)}>
                {t[0].toUpperCase() + t.slice(1)}
              </button>
            ))}
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
          <div className="lbl"><b>Agent detection</b><span>Badge and sidebar dot follow the process tree, polled every 3 s</span></div>
          <span className="mono">{AGENTS.join(" · ")}</span>
        </div>
        <div className="srow">
          <div className="lbl"><b>Task files</b><span>Per-project source of truth, git-trackable; the Inbox lives in veal.db</span></div>
          <span className="mono">.veal/tasks.json</span>
        </div>
        <div className="srow">
          <div className="lbl"><b>File watching</b><span>Every pinned root, recursively; Refresh still re-lists by hand</span></div>
          <span className="mono">notify</span>
        </div>
        <div className="srow" style={{ border: 0 }}>
          <div className="lbl"><b>Restore layout on launch</b><span>Reopens panes, cwds and weights, and each project's file tree</span></div>
          <label>
            <input type="checkbox" checked={restoreLayout} onChange={(e) => setRestoreLayout(e.target.checked)} /> Enabled
          </label>
        </div>
      </div>
    </>
  );
}
