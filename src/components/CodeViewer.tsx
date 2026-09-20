// CodeMirror viewer/editor (plan-04, plan-06). One EditorView per mount.
// Files open read-only; the head's toggle turns editing on per file, and
// becomes Save once the file is dirty. A file's document is rebuilt only when
// a read replaces it (`gen`), so saves never reset the cursor; dirty files
// stash their EditorState on switch and get it back with undo intact.
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, indentOnInput } from "@codemirror/language";
import { openSearchPanel, search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { drawSelection, EditorView, highlightActiveLine, highlightSpecialChars, keymap, lineNumbers } from "@codemirror/view";
import { useEffect, useRef, useState, type RefObject } from "react";
import { languageFor } from "../files/lang";
import { useFiles, type FileState } from "../files/store";
import { viewerHighlight, viewerTheme } from "../files/theme";
import { formatSize, pathKey } from "../files/tree";

const READ_CAP_LABEL = "2 MB";

/** Nothing is shown for reads shorter than this; a flash of "Loading…" is worse than a blank. */
const LOADING_DELAY_MS = 150;

interface Props {
  /** True while the Files view is showing; Ctrl+F / Ctrl+S anywhere in it then reach the editor. */
  active: boolean;
  projectId: number;
  projectName: string;
  /** Absolute path of the open file. */
  path: string | null;
  /** Path relative to the project root, for the head. */
  rel: string | null;
  file: FileState | null;
  editing: boolean;
  dirty: boolean;
}

export function CodeViewer({ active, projectId, projectName, path, rel, file, editing, dirty }: Props) {
  const setEditing = useFiles((s) => s.setEditing);
  const discard = useFiles((s) => s.discard);
  const save = useFiles((s) => s.save);
  const viewRef = useRef<EditorView | null>(null);

  const loaded = file?.status === "loaded" ? file : null;
  const doSave = (force = false) => {
    const v = viewRef.current;
    if (!path || !v || !dirty || loaded?.saving) return;
    void save(projectId, path, v.state.doc.toString(), force);
  };
  const saveRef = useRef(doSave);
  saveRef.current = doSave;

  // Ctrl+S / Ctrl+F from the tree (or anywhere in the view) reach the editor
  // instead of WebView2's own "Save page" dialog / find bar. Ctrl+S is
  // swallowed even with no file open, so the dialog can never appear here.
  useEffect(() => {
    if (!active) return;
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (!(e.ctrlKey && !e.altKey && !e.metaKey)) return;
      const k = e.key.toLowerCase();
      if (k === "s") {
        e.preventDefault();
        saveRef.current();
        return;
      }
      if (k !== "f") return;
      const v = viewRef.current;
      if (!v || v.dom.contains(e.target as Node)) return; // the editor's own keymap handles it
      e.preventDefault();
      v.focus();
      openSearchPanel(v);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);

  const pill = !loaded
    ? null
    : loaded.saving
      ? { text: "saving…", cls: "" }
      : loaded.conflict
        ? { text: "conflict", cls: " agent" }
        : loaded.lossy
          ? { text: "not UTF-8", cls: " agent" }
          : dirty
          ? { text: "modified", cls: " accent" }
          : editing
            ? null
            : { text: "read-only", cls: "" };

  return (
    <div className="viewer">
      {rel ? (
        <div className="viewer-head">
          <span className="viewer-path">{projectName} / <b>{rel}</b></span>
          <span className="grow" />
          {loaded && <span>{formatSize(loaded.size)}</span>}
          {loaded && path && (
            <>
              {dirty && (
                <button className="btn ghost" onClick={() => discard(projectId, path)} disabled={loaded.saving}>
                  Discard
                </button>
              )}
              {dirty ? (
                <button className="btn primary" onClick={() => doSave()} disabled={loaded.saving}>
                  Save
                </button>
              ) : (
                <button
                  className="btn"
                  onClick={() => setEditing(projectId, path, !editing)}
                  disabled={loaded.lossy}
                  title={loaded.lossy ? "Not UTF-8: shown lossily, cannot be saved from Veal" : undefined}
                >
                  {editing ? "Done" : "Edit"}
                </button>
              )}
            </>
          )}
          {pill && <span className={`pill${pill.cls}`}>{pill.text}</span>}
        </div>
      ) : (
        <div className="viewer-head hint">No file open</div>
      )}
      {loaded?.conflict && path && (
        <div className="viewer-banner">
          <span>{loaded.conflict === "changed" ? "Changed on disk since you opened it." : "Deleted on disk."}</span>
          <span className="grow" />
          <button className="btn" onClick={() => doSave(true)} disabled={loaded.saving}>Overwrite</button>
          <button className="btn ghost" onClick={() => discard(projectId, path)} disabled={loaded.saving}>Reload</button>
        </div>
      )}
      {loaded && path && rel ? (
        <Editor
          projectId={projectId}
          path={path}
          name={rel}
          text={loaded.text}
          gen={loaded.gen}
          editing={editing}
          dirty={dirty}
          viewRef={viewRef}
          onSave={() => doSave()}
        />
      ) : (
        <ViewerMessage rel={rel} file={file} />
      )}
    </div>
  );
}

function ViewerMessage({ rel, file }: { rel: string | null; file: FileState | null }) {
  const [slow, setSlow] = useState(false);
  useEffect(() => {
    setSlow(false);
    if (file?.status !== "loading") return;
    const t = setTimeout(() => setSlow(true), LOADING_DELAY_MS);
    return () => clearTimeout(t);
  }, [file]);

  if (!rel || !file) {
    return (
      <div className="viewer-empty">
        Pick a file from the tree.
        <br />
        <span className="hint">Edit to change a file · Ctrl+F searches</span>
      </div>
    );
  }
  if (file.status === "loading") return <div className="viewer-empty">{slow ? "Loading…" : ""}</div>;
  if (file.status === "error") return <div className="viewer-empty">{describeError(file.error)}</div>;
  return null;
}

function describeError(error: string): string {
  if (error === "binary") return "Binary file — not shown";
  const big = error.match(/^too large: (\d+) bytes$/);
  if (big) return `${formatSize(Number(big[1]))} — the viewer caps at ${READ_CAP_LABEL}`;
  return error;
}

const baseExtensions: Extension = [
  lineNumbers(),
  drawSelection(),
  highlightSpecialChars(),
  history(),
  bracketMatching(),
  indentOnInput(),
  search({ top: true }),
  keymap.of([...searchKeymap, ...historyKeymap, indentWithTab, ...defaultKeymap]),
  // A non-editable content DOM gets no tabindex from CodeMirror, and without
  // focus none of the keymaps (search, movement) would ever fire.
  EditorView.contentAttributes.of({ tabindex: "0" }),
  viewerTheme,
  viewerHighlight,
];

const readOnlyExt: Extension = [EditorState.readOnly.of(true), EditorView.editable.of(false)];
const editableExt: Extension = [highlightActiveLine()];

// One EditorView exists at a time, and a stashed EditorState must be
// reconfigurable by whichever Editor instance restores it, so the
// compartments are module-wide rather than per mount.
const langCompartment = new Compartment();
const modeCompartment = new Compartment();

interface EditorProps {
  projectId: number;
  path: string;
  name: string;
  text: string;
  gen: number;
  editing: boolean;
  dirty: boolean;
  viewRef: RefObject<EditorView | null>;
  onSave(): void;
}

function Editor({ projectId, path, name, text, gen, editing, dirty, viewRef, onSave }: EditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const markDirty = useFiles((s) => s.markDirty);
  const stash = useFiles((s) => s.stash);
  // What the live view currently shows; the effect below stashes it before moving on.
  const shown = useRef<{ projectId: number; path: string } | null>(null);
  const latest = useRef({ editing, dirty, onSave, projectId, path });
  latest.current = { editing, dirty, onSave, projectId, path };

  useEffect(() => {
    const v = new EditorView({ parent: host.current! });
    viewRef.current = v;
    return () => {
      const cur = shown.current;
      if (cur && latest.current.dirty) stash(cur.projectId, cur.path, v.state);
      v.destroy();
      viewRef.current = null;
    };
  }, [viewRef, stash]);

  // New document: on file switch or when a read replaced the text (`gen`).
  // A dirty file coming back gets its stashed state; anything else is fresh.
  useEffect(() => {
    const v = viewRef.current;
    if (!v) return;
    const prev = shown.current;
    if (prev && (prev.projectId !== projectId || pathKey(prev.path) !== pathKey(path))) {
      const wasDirty = useFiles.getState().byProject.get(prev.projectId)?.drafts.has(pathKey(prev.path));
      if (wasDirty) stash(prev.projectId, prev.path, v.state);
    }
    shown.current = { projectId, path };

    const draft = useFiles.getState().byProject.get(projectId)?.drafts.get(pathKey(path));
    const extensions = [
      baseExtensions,
      modeCompartment.of(latest.current.editing ? editableExt : readOnlyExt),
      langCompartment.of([]),
      EditorView.updateListener.of((u) => {
        if (u.docChanged && !latest.current.dirty) markDirty(latest.current.projectId, latest.current.path);
      }),
      keymap.of([{ key: "Mod-s", run: () => (latest.current.onSave(), true) }]),
    ];
    if (draft?.state) {
      // The stashed state was built with these same compartments, so only the
      // mode needs syncing; the language below is re-applied harmlessly.
      v.setState(draft.state);
      v.dispatch({ effects: modeCompartment.reconfigure(latest.current.editing ? editableExt : readOnlyExt) });
    } else {
      v.setState(EditorState.create({ doc: text, extensions }));
    }
    const desc = languageFor(name);
    if (!desc) return;
    let live = true;
    desc.load().then((support) => {
      // The state may have been replaced by the next file while the chunk loaded.
      if (live && viewRef.current === v && shown.current?.path === path) {
        v.dispatch({ effects: langCompartment.reconfigure(support) });
      }
    }, (e) => console.warn("language", desc.name, e));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, path, gen]);

  // Edit toggle: swap the compartment, keep scroll/selection/history.
  useEffect(() => {
    const v = viewRef.current;
    if (!v) return;
    v.dispatch({ effects: modeCompartment.reconfigure(editing ? editableExt : readOnlyExt) });
    if (editing) v.focus();
  }, [editing, viewRef]);

  return <div className="code" ref={host} />;
}
