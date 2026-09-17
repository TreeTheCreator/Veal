// Files view (plan-04, plan-06, plan-09): the active project's directory
// tree on the left, a CodeMirror viewer/editor on the right. Per-project
// state — tree, open file, edit mode, drafts — lives in files/store.ts and
// survives switching projects. This component owns the tree's context menu
// and the one modal (deleting unsaved edits).
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { EntryKind } from "../fs";
import { selectProject, useFiles, type DirtyFile } from "../files/store";
import { flatten, pathKey, type Row } from "../files/tree";
import { parentPath } from "../files/watch";
import { basename, normalizePath, tilde } from "../paths";
import { selectActive, useProjects } from "../projects/store";
import { CodeViewer } from "./CodeViewer";
import { FileTree } from "./FileTree";

interface Menu {
  x: number;
  y: number;
  /** null: the root. */
  row: Row | null;
}

interface PendingDelete {
  row: Row;
  dirty: DirtyFile[];
}

export function FilesView({ visible }: { visible: boolean }) {
  const active = useProjects(selectActive);
  const loaded = useProjects((s) => s.loaded);
  const state = useFiles(selectProject(active?.id ?? null));
  const edit = useFiles((s) => s.edit);
  const ensure = useFiles((s) => s.ensure);
  const toggleDir = useFiles((s) => s.toggleDir);
  const openFile = useFiles((s) => s.openFile);
  const refresh = useFiles((s) => s.refresh);

  const [menu, setMenu] = useState<Menu | null>(null);
  const [pending, setPending] = useState<PendingDelete | null>(null);
  // The focused tree row, for the toolbar's ＋ buttons (a ref: it changes on every arrow key).
  const cursorRow = useRef<Row | null>(null);
  const onCursor = useCallback((row: Row | null) => {
    cursorRow.current = row;
  }, []);

  // List the root the first time this project is looked at.
  useEffect(() => {
    if (visible && active) ensure(active.id, active.path);
  }, [visible, active, ensure]);

  // The context menu closes on any click elsewhere or Escape, like the Sidebar's.
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", onKey);
    };
  }, [menu]);

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
  const root = active.path;
  const rootState = state?.dirs.get(pathKey(root));
  const rootError = rootState?.status === "error" ? rootState.error : null;
  const open = state?.open ?? null;
  const openKey = open ? pathKey(open) : null;
  const rel = open ? relativeTo(root, open) : null;
  const dirty = state?.drafts ?? new Map();
  const myEdit = edit?.projectId === id ? edit : null;

  /** The directory a row stands for: itself for a directory, its parent for a file, the root for none. */
  const dirOf = (row: Row | null): string => (row ? (row.kind === "dir" ? row.path : (parentPath(row.path) ?? root)) : root);

  const startCreate = (row: Row | null, kind: EntryKind) => {
    setMenu(null);
    useFiles.getState().startCreate(id, dirOf(row), kind);
  };
  const startRename = (row: Row) => {
    setMenu(null);
    useFiles.getState().startRename(id, row.path);
  };
  const requestDelete = (row: Row) => {
    setMenu(null);
    const unsaved = useFiles.getState().dirtyUnder(id, row.path);
    if (unsaved.length) setPending({ row, dirty: unsaved });
    else void useFiles.getState().remove(id, row.path);
  };
  const confirmDelete = () => {
    if (!pending) return;
    const { row } = pending;
    setPending(null);
    void useFiles.getState().remove(id, row.path);
  };
  const commit = (name: string): Promise<boolean> => {
    const s = useFiles.getState();
    if (!myEdit) return Promise.resolve(false);
    return myEdit.mode === "create" ? s.create(id, myEdit.dir, name, myEdit.kind) : s.rename(id, myEdit.path, name);
  };

  return (
    <>
      <div className="toolbar">
        <span>Root</span>
        <span className="mono" style={{ color: "var(--text)" }}>{tilde(root)}</span>
        <button className="btn" onClick={() => refresh(id)} title="Re-list expanded folders and re-read the open file">
          Refresh
        </button>
        {/* mousedown is prevented so the tree keeps focus and its row stays the target. */}
        <button
          className="btn"
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => startCreate(cursorRow.current, "file")}
          title="New file in the focused folder (root when none)"
        >
          ＋ File
        </button>
        <button
          className="btn"
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => startCreate(cursorRow.current, "dir")}
          title="New folder in the focused folder (root when none)"
        >
          ＋ Folder
        </button>
        <span className="grow" />
        <span className="hint">Right-click a row · F2 renames · Del moves to the Recycle Bin · Ctrl+S saves</span>
      </div>
      <div className="files">
        <FileTree
          rows={rows}
          open={open}
          dirty={dirty}
          rootError={rootError}
          edit={myEdit}
          onToggle={(p) => toggleDir(id, p)}
          onOpen={(p) => openFile(id, p)}
          onCommit={commit}
          onCancelEdit={() => useFiles.getState().cancelEdit()}
          onContextMenu={(row, x, y) => setMenu({ row, x, y })}
          onRename={startRename}
          onDelete={requestDelete}
          onCursor={onCursor}
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

      {menu && (
        <div className="ctx-menu" role="menu" style={clampMenu(menu.x, menu.y, menu.row ? 4 : 2)} onPointerDown={(e) => e.stopPropagation()}>
          <button role="menuitem" onClick={() => startCreate(menu.row, "file")}>New file</button>
          <button role="menuitem" onClick={() => startCreate(menu.row, "dir")}>New folder</button>
          {menu.row && (
            <>
              <button role="menuitem" onClick={() => startRename(menu.row!)}>Rename</button>
              <button role="menuitem" onClick={() => requestDelete(menu.row!)}>Delete</button>
            </>
          )}
        </div>
      )}

      {pending && (
        <div className="scrim open" onPointerDown={(e) => e.target === e.currentTarget && setPending(null)}>
          <div className="modal" role="dialog" aria-modal="true">
            <h3>Delete {basename(pending.row.path)}?</h3>
            <div className="field">
              <span>
                {pending.row.kind === "dir"
                  ? `${pending.dirty.length} file${pending.dirty.length === 1 ? "" : "s"} under it ${pending.dirty.length === 1 ? "has" : "have"} unsaved changes.`
                  : "It has unsaved changes."}{" "}
                The file goes to the Recycle Bin; the edits do not.
              </span>
              <ul className="dirty-list mono">
                {pending.dirty.map((d) => (
                  <li key={d.path}>{relativeTo(root, d.path)}</li>
                ))}
              </ul>
            </div>
            <div className="row">
              <button type="button" className="btn ghost" onClick={() => setPending(null)} autoFocus>
                Cancel
              </button>
              <button type="button" className="btn" onClick={confirmDelete}>
                Delete anyway
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

/** Keeps a context menu of `items` rows inside the window, so a row near the bottom edge still shows every item. */
function clampMenu(x: number, y: number, items: number): { left: number; top: number } {
  const width = 170;
  const height = items * 32 + 10;
  return {
    left: Math.max(0, Math.min(x, window.innerWidth - width)),
    top: Math.max(0, Math.min(y, window.innerHeight - height)),
  };
}

/** `C:\Users\user\veal\src\App.tsx` under `C:\Users\user\veal` → `src/App.tsx`. */
export function relativeTo(root: string, path: string): string {
  const r = normalizePath(root);
  const p = normalizePath(path);
  const rel = p.toLowerCase().startsWith(r.toLowerCase()) ? p.slice(r.length).replace(/^[\\/]+/, "") : p;
  return rel.replace(/\\/g, "/");
}
