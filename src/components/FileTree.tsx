// The directory tree as a flat list of rows (files/tree.ts). Click toggles a
// directory or opens a file; arrows move, → expands, ← collapses or goes to
// the parent, Enter opens — all index arithmetic over the rows.
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Icon } from "../Icon";
import { parentIndex, pathKey, type Row } from "../files/tree";

interface Props {
  rows: Row[];
  open: string | null;
  /** Files with unsaved edits, keyed by pathKey. */
  dirty: ReadonlyMap<string, unknown>;
  rootError: string | null;
  onToggle(path: string): void;
  onOpen(path: string): void;
}

const INDENT_PX = 16;

export function FileTree({ rows, open, dirty, rootError, onToggle, onOpen }: Props) {
  const [cursor, setCursor] = useState(0);
  const host = useRef<HTMLDivElement>(null);
  const openKey = open ? pathKey(open) : null;

  // Keep the cursor on a real row as the list changes underneath it.
  useEffect(() => {
    if (cursor >= rows.length) setCursor(Math.max(0, rows.length - 1));
  }, [rows.length, cursor]);

  const activate = (row: Row) => (row.kind === "dir" ? onToggle(row.path) : onOpen(row.path));

  const moveTo = (i: number) => {
    setCursor(i);
    const el = host.current?.querySelectorAll<HTMLElement>(".node")[i];
    el?.focus();
    el?.scrollIntoView({ block: "nearest" });
  };

  const onKey = (e: KeyboardEvent, i: number) => {
    const row = rows[i];
    if (!row) return;
    switch (e.key) {
      case "ArrowDown":
        if (i + 1 < rows.length) moveTo(i + 1);
        break;
      case "ArrowUp":
        if (i > 0) moveTo(i - 1);
        break;
      case "ArrowRight":
        if (row.kind === "dir" && !row.expanded) onToggle(row.path);
        else if (row.kind === "dir" && i + 1 < rows.length && rows[i + 1].depth > row.depth) moveTo(i + 1);
        break;
      case "ArrowLeft":
        if (row.kind === "dir" && row.expanded) onToggle(row.path);
        else {
          const parent = parentIndex(rows, i);
          if (parent >= 0) moveTo(parent);
        }
        break;
      case "Enter":
      case " ":
        activate(row);
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  return (
    <div className="tree" ref={host} role="tree">
      {rootError && <div className="tree-error">{rootError}</div>}
      {rows.map((row, i) => {
        const isDir = row.kind === "dir";
        const key = pathKey(row.path);
        const isOpen = !isDir && openKey === key;
        const isDirty = !isDir && dirty.has(key);
        const cls = ["node", isDir ? "dir" : "file", isDir && !row.expanded ? "collapsed" : "", isOpen ? "open-file" : ""]
          .filter(Boolean)
          .join(" ");
        return (
          <div key={row.path} style={{ paddingLeft: row.depth * INDENT_PX }}>
            <button
              className={cls}
              role="treeitem"
              aria-expanded={isDir ? row.expanded : undefined}
              aria-selected={isOpen}
              tabIndex={i === cursor ? 0 : -1}
              title={isDirty ? `${row.path} — unsaved changes` : row.path}
              onClick={() => {
                setCursor(i);
                activate(row);
              }}
              onKeyDown={(e) => onKey(e, i)}
            >
              <span className={`tw${row.expanded && row.state === "loading" ? " busy" : ""}`}>{isDir ? "▼" : ""}</span>
              <Icon name={isDir ? "folder" : "file"} size={14} />
              <span className="nname">{row.name}</span>
              {isDirty && <span className="dirty" aria-label="unsaved changes" />}
            </button>
            {row.error && row.expanded && <div className="tree-error">{row.error}</div>}
          </div>
        );
      })}
    </div>
  );
}
