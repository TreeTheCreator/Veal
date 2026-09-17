// Stops the window closing over unsaved edits (plan-06). Tauri prevents the
// close while this listener exists and lets us decide; Rust kills the PTYs
// on Destroyed, not CloseRequested, so a cancelled close keeps the panes.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { useFiles, type DirtyFile } from "../files/store";
import { useProjects } from "../projects/store";
import { relativeTo } from "./FilesView";

export function CloseGuard() {
  const [pending, setPending] = useState<DirtyFile[] | null>(null);

  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onCloseRequested((e) => {
      const dirty = useFiles.getState().dirtyFiles();
      if (dirty.length === 0) return;
      e.preventDefault();
      setPending(dirty);
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
    for (const d of pending) useFiles.getState().discard(d.projectId, d.path);
    getCurrentWindow().destroy().catch((e) => console.error("destroy", e));
  };

  return (
    <div className="scrim open" onPointerDown={(e) => e.target === e.currentTarget && setPending(null)}>
      <div className="modal" role="dialog" aria-modal="true">
        <h3>Unsaved changes</h3>
        <div className="field">
          <span>These files have edits that have not been saved. Ctrl+S them first, or discard them.</span>
          <ul className="dirty-list mono">
            {pending.map((d) => (
              <li key={`${d.projectId}:${d.path}`}>{label(d)}</li>
            ))}
          </ul>
        </div>
        <div className="row">
          <button type="button" className="btn ghost" onClick={() => setPending(null)} autoFocus>
            Cancel
          </button>
          <button type="button" className="btn" onClick={quit}>
            Discard and quit
          </button>
        </div>
      </div>
    </div>
  );
}
