// Stops the window closing over unsaved edits (plan-06), and decides what
// happens to running agents (plan-19 section 8): keep them running in the
// background service, or stop them. Tauri prevents the close while this
// listener exists and lets us decide; Rust acts on Destroyed, not
// CloseRequested, so a cancelled close keeps the panes. Every close is
// intercepted so the debounced layout and tree saves can be flushed first
// (plan-16).
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { flushFiles } from "../files/persist";
import { useFiles, type DirtyFile } from "../files/store";
import { flushLayout } from "../layout/persist";
import { useLayout } from "../layout/store";
import { closeDecision } from "../close";
import { runningAgents, useProjects, type RunningAgent } from "../projects/store";
import { setCloseMode } from "../pty";
import { useSettings } from "../settings/store";
import { relativeTo } from "./FilesView";

/** A stuck IPC must not pin the window open. */
const FLUSH_TIMEOUT_MS = 1000;

interface Pending {
  dirty: DirtyFile[];
  agents: RunningAgent[];
}

export function flushAll(): Promise<void> {
  const flushes = Promise.all([flushLayout(), flushFiles()]).then(() => {});
  const timeout = new Promise<void>((r) => setTimeout(r, FLUSH_TIMEOUT_MS));
  return Promise.race([flushes, timeout]);
}

/** Flush, tell Rust whether the shells stay, then let the window go. */
function close(keep: boolean): void {
  const mode = setCloseMode(keep).catch((e) => console.error("pty_close_mode", e));
  Promise.all([mode, flushAll()])
    .then(() => getCurrentWindow().destroy())
    .catch((e) => console.error("destroy", e));
}

export function CloseGuard() {
  const [pending, setPending] = useState<Pending | null>(null);
  const [remember, setRemember] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onCloseRequested((e) => {
      e.preventDefault();
      const dirty = useFiles.getState().dirtyFiles();
      const { tree, agents: detected } = useLayout.getState();
      const agents = runningAgents(tree, useProjects.getState().projects, detected);
      const decision = closeDecision(dirty.length, agents.length, useSettings.getState().closeWithAgents);
      if (decision !== "ask") {
        close(decision === "keep");
        return;
      }
      setRemember(false);
      setPending({ dirty, agents });
    });
    return () => {
      unlisten.then((f) => f()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (!pending) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setPending(null);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [pending]);

  if (!pending) return null;
  const projects = useProjects.getState().projects;
  const label = (d: DirtyFile) => {
    const name = projects.find((p) => p.id === d.projectId)?.name ?? "?";
    return `${name} / ${relativeTo(d.root, d.path)}`;
  };
  const { dirty, agents } = pending;
  const quit = (keep: boolean) => {
    for (const d of dirty) useFiles.getState().discard(d.projectId, d.path);
    if (remember && agents.length > 0) useSettings.getState().setCloseWithAgents(keep ? "keep" : "stop");
    close(keep);
  };
  const title = dirty.length && agents.length ? "Unsaved changes and running agents" : dirty.length ? "Unsaved changes" : "Agents running";

  return (
    <div className="scrim open" onPointerDown={(e) => e.target === e.currentTarget && setPending(null)}>
      <div className="modal" role="dialog" aria-modal="true">
        <h3>{title}</h3>
        {dirty.length > 0 && (
          <div className="field">
            <span>These files have edits that have not been saved. Ctrl+S them first, or discard them.</span>
            <ul className="dirty-list mono">
              {dirty.map((d) => (
                <li key={`${d.projectId}:${d.path}`}>{label(d)}</li>
              ))}
            </ul>
          </div>
        )}
        {agents.length > 0 && (
          <div className="field">
            <span>These can keep running in the background, with Veal in the tray, or stop now.</span>
            <ul className="dirty-list mono">
              {agents.map((a) => (
                <li key={a.paneId}>
                  {a.agent} in {a.where}
                </li>
              ))}
            </ul>
          </div>
        )}
        {agents.length > 0 && (
          <label className="remember">
            <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} />
            Don't ask again (change it in Settings)
          </label>
        )}
        <div className="row">
          {agents.length > 0 ? (
            <>
              <button type="button" className="btn ghost" onClick={() => setPending(null)}>
                Cancel
              </button>
              <button type="button" className="btn" onClick={() => quit(false)}>
                {dirty.length ? "Discard and stop" : "Stop and quit"}
              </button>
              <button type="button" className="btn primary" onClick={() => quit(true)} autoFocus>
                {dirty.length ? "Discard and keep running" : "Keep running"}
              </button>
            </>
          ) : (
            <>
              <button type="button" className="btn ghost" onClick={() => setPending(null)} autoFocus>
                Cancel
              </button>
              <button type="button" className="btn" onClick={() => quit(false)}>
                Discard and quit
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
