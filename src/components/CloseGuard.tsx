// Stops the window closing over unsaved edits (plan-06) or a running agent
// (plan-16). Tauri prevents the close while this listener exists and lets
// us decide; Rust kills the PTYs on Destroyed, not CloseRequested, so a
// cancelled close keeps the panes. Every close is intercepted so the
// debounced layout and tree saves can be flushed first (plan-16).
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { flushFiles } from "../files/persist";
import { useFiles, type DirtyFile } from "../files/store";
import { flushLayout } from "../layout/persist";
import { useLayout } from "../layout/store";
import { runningAgents, useProjects, type RunningAgent } from "../projects/store";
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

export function CloseGuard() {
  const [pending, setPending] = useState<Pending | null>(null);

  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onCloseRequested((e) => {
      e.preventDefault();
      const dirty = useFiles.getState().dirtyFiles();
      const { tree, agents: detected } = useLayout.getState();
      const agents = runningAgents(tree, useProjects.getState().projects, detected);
      if (dirty.length === 0 && agents.length === 0) {
        flushAll().then(() => win.destroy()).catch((e) => console.error("destroy", e));
        return;
      }
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
  const quit = () => {
    for (const d of pending.dirty) useFiles.getState().discard(d.projectId, d.path);
    flushAll()
      .then(() => getCurrentWindow().destroy())
      .catch((e) => console.error("destroy", e));
  };
  const { dirty, agents } = pending;
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
            <span>Quitting ends these sessions.</span>
            <ul className="dirty-list mono">
              {agents.map((a) => (
                <li key={a.paneId}>
                  {a.agent} in {a.where}
                </li>
              ))}
            </ul>
          </div>
        )}
        <div className="row">
          <button type="button" className="btn ghost" onClick={() => setPending(null)} autoFocus>
            Cancel
          </button>
          <button type="button" className="btn" onClick={quit}>
            {dirty.length ? "Discard and quit" : "Quit anyway"}
          </button>
        </div>
      </div>
    </div>
  );
}
