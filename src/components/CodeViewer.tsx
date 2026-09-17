// Read-only CodeMirror viewer (plan-04). One EditorView per mount; each file
// becomes a fresh EditorState so scroll and selection start clean. The
// language chunk is lazy-loaded and swapped in through a Compartment.
import { defaultKeymap } from "@codemirror/commands";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { openSearchPanel, search, searchKeymap } from "@codemirror/search";
import { drawSelection, EditorView, highlightSpecialChars, keymap, lineNumbers } from "@codemirror/view";
import { useEffect, useRef, useState } from "react";
import { languageFor } from "../files/lang";
import type { FileState } from "../files/store";
import { viewerHighlight, viewerTheme } from "../files/theme";
import { formatSize } from "../files/tree";

const READ_CAP_LABEL = "2 MB";

/** Nothing is shown for reads shorter than this; a flash of "Loading…" is worse than a blank. */
const LOADING_DELAY_MS = 150;

interface Props {
  /** True while the Files view is showing; Ctrl+F anywhere in it then opens the viewer's search. */
  active: boolean;
  projectName: string;
  /** Path relative to the project root, for the head. */
  rel: string | null;
  file: FileState | null;
}

export function CodeViewer({ active, projectName, rel, file }: Props) {
  const text = file?.status === "loaded" ? file.text : null;
  return (
    <div className="viewer">
      {rel ? (
        <div className="viewer-head">
          <span>{projectName} / <b>{rel}</b></span>
          <span className="grow" />
          {file?.status === "loaded" && <span>{formatSize(file.size)}</span>}
          <span className="pill">read-only</span>
        </div>
      ) : (
        <div className="viewer-head hint">No file open</div>
      )}
      {text !== null && rel ? <Editor name={rel} text={text} active={active} /> : <ViewerMessage rel={rel} file={file} />}
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
        <span className="hint">Opens read-only; editing comes later.</span>
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
  search({ top: true }),
  keymap.of([...searchKeymap, ...defaultKeymap]),
  EditorState.readOnly.of(true),
  EditorView.editable.of(false),
  // A non-editable content DOM gets no tabindex from CodeMirror, and without
  // focus none of the keymaps (search, movement) would ever fire.
  EditorView.contentAttributes.of({ tabindex: "0" }),
  viewerTheme,
  viewerHighlight,
];

function Editor({ name, text, active }: { name: string; text: string; active: boolean }) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);

  // Ctrl+F from the tree (or anywhere in the view) searches the file instead
  // of opening WebView2's own find bar.
  useEffect(() => {
    if (!active) return;
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (!(e.ctrlKey && !e.altKey && !e.metaKey && e.key.toLowerCase() === "f")) return;
      const v = view.current;
      if (!v || v.dom.contains(e.target as Node)) return; // the editor's own keymap handles it
      e.preventDefault();
      v.focus();
      openSearchPanel(v);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);

  useEffect(() => {
    const v = new EditorView({ parent: host.current! });
    view.current = v;
    return () => {
      v.destroy();
      view.current = null;
    };
  }, []);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    const lang = new Compartment();
    v.setState(EditorState.create({ doc: text, extensions: [baseExtensions, lang.of([])] }));
    const desc = languageFor(name);
    if (!desc) return;
    let live = true;
    desc.load().then((support) => {
      // The state may have been replaced by the next file while the chunk loaded.
      if (live && view.current === v) v.dispatch({ effects: lang.reconfigure(support) });
    }, (e) => console.warn("language", desc.name, e));
    return () => {
      live = false;
    };
  }, [name, text]);

  return <div className="code" ref={host} />;
}
