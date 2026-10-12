// Settings sheet (mockup `#view-settings`). Every control writes through to
// veal.db on change; read-only rows state what the app currently does.
import { homeDir } from "@tauri-apps/api/path";
import { useEffect, useState } from "react";
import { Icon } from "../Icon";
import { PANE_CAP } from "../layout/tree";
import { normalizePath, tilde } from "../paths";
import { AGENTS } from "../pty";
import { parsePort, SHELL_CHOICES, useSettings, type ShellChoice } from "../settings/store";
import { TAB_LABEL, useUi } from "../ui";

export function SettingsView() {
  const { theme, shell, restoreLayout, proxyPort, setTheme, setShell, setRestoreLayout, setProxyPort } = useSettings();
  const lastTab = useUi((s) => s.lastTab);
  const setView = useUi((s) => s.setView);
  const portBad = proxyPort.trim() !== "" && parsePort(proxyPort) === null;
  const [home, setHome] = useState("~");

  useEffect(() => {
    homeDir().then((h) => setHome(tilde(normalizePath(h)))).catch(() => {});
  }, []);

  return (
    <>
      <div className="toolbar">
        <button className="back-btn" onClick={() => setView(lastTab)} title={`Back to ${TAB_LABEL[lastTab]}`} aria-label={`Back to ${TAB_LABEL[lastTab]}`}>
          <Icon name="back" />
        </button>
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
          <div className="lbl">
            <b>Proxy port</b>
            <span>
              {portBad
                ? "Not a valid port (1–65535); new panes start without a proxy"
                : "Sets http_proxy and https_proxy to 127.0.0.1:port in new panes; blank for none"}
            </span>
          </div>
          <input
            className="mono port"
            inputMode="numeric"
            placeholder="None"
            value={proxyPort}
            aria-invalid={portBad}
            onChange={(e) => setProxyPort(e.target.value.replace(/\D/g, "").slice(0, 5))}
          />
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
