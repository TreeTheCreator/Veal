// Files view (plan-04): the active project's directory tree on the left, a
// read-only CodeMirror viewer on the right. Per-project state lives in
// files/store.ts and survives switching projects.
import { useEffect, useMemo } from "react";
import { selectProject, useFiles } from "../files/store";
import { flatten, pathKey } from "../files/tree";
import { normalizePath, tilde } from "../paths";
import { selectActive, useProjects } from "../projects/store";
import { CodeViewer } from "./CodeViewer";
import { FileTree } from "./FileTree";

export function FilesView({ visible }: { visible: boolean }) {
  const active = useProjects(selectActive);
  const loaded = useProjects((s) => s.loaded);
  const state = useFiles(selectProject(active?.id ?? null));
  const ensure = useFiles((s) => s.ensure);
  const toggleDir = useFiles((s) => s.toggleDir);
  const openFile = useFiles((s) => s.openFile);
  const refresh = useFiles((s) => s.refresh);

  // List the root the first time this project is looked at.
  useEffect(() => {
    if (visible && active) ensure(active.id, active.path);
  }, [visible, active, ensure]);

  const rows = useMemo(
    () => (state ? flatten(state.root, state.dirs, state.expanded) : []),
    [state],
  );

  if (!active) {
    return (
      <>
        <div className="toolbar">
          <span>Root</span>
          <span className="mono" style={{ color: "var(--text)" }}>—</span>
        </div>
        <div className="empty">{loaded ? "Pin a project to browse its files." : ""}</div>
      </>
    );
  }

  const id = active.id;
  const rootState = state?.dirs.get(pathKey(active.path));
  const rootError = rootState?.status === "error" ? rootState.error : null;
  const rel = state?.open ? relativeTo(active.path, state.open) : null;

  return (
    <>
      <div className="toolbar">
        <span>Root</span>
        <span className="mono" style={{ color: "var(--text)" }}>{tilde(active.path)}</span>
        <button className="btn" onClick={() => refresh(id)} title="Re-list expanded folders and re-read the open file">
          Refresh
        </button>
        <span className="grow" />
        <span className="hint">Read-only · Ctrl+F searches the file</span>
      </div>
      <div className="files">
        <FileTree
          rows={rows}
          open={state?.open ?? null}
          rootError={rootError}
          onToggle={(p) => toggleDir(id, p)}
          onOpen={(p) => openFile(id, p)}
        />
        <CodeViewer active={visible} projectName={active.name} rel={rel} file={state?.file ?? null} />
      </div>
    </>
  );
}

/** `C:\Users\user\veal\src\App.tsx` under `C:\Users\user\veal` → `src/App.tsx`. */
export function relativeTo(root: string, path: string): string {
  const r = normalizePath(root);
  const p = normalizePath(path);
  const rel = p.toLowerCase().startsWith(r.toLowerCase()) ? p.slice(r.length).replace(/^[\\/]+/, "") : p;
  return rel.replace(/\\/g, "/");
}
