// Files view (plan-04, plan-06): the active project's directory tree on the
// left, a CodeMirror viewer/editor on the right. Per-project state — tree,
// open file, edit mode, drafts — lives in files/store.ts and survives
// switching projects.
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
  const open = state?.open ?? null;
  const openKey = open ? pathKey(open) : null;
  const rel = open ? relativeTo(active.path, open) : null;
  const dirty = state?.drafts ?? new Map();

  return (
    <>
      <div className="toolbar">
        <span>Root</span>
        <span className="mono" style={{ color: "var(--text)" }}>{tilde(active.path)}</span>
        <button className="btn" onClick={() => refresh(id)} title="Re-list expanded folders and re-read the open file">
          Refresh
        </button>
        <span className="grow" />
        <span className="hint">Edit to change a file · Ctrl+S saves · Ctrl+F searches</span>
      </div>
      <div className="files">
        <FileTree
          rows={rows}
          open={open}
          dirty={dirty}
          rootError={rootError}
          onToggle={(p) => toggleDir(id, p)}
          onOpen={(p) => openFile(id, p)}
        />
        <CodeViewer
          active={visible}
          projectId={id}
          projectName={active.name}
          path={open}
          rel={rel}
          file={state?.file ?? null}
          editing={openKey !== null && (state?.editing.has(openKey) ?? false)}
          dirty={openKey !== null && dirty.has(openKey)}
        />
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
