// The directory tree as a flat list of rows (files/tree.ts). Click toggles a
// directory or opens a file; arrows move, → expands, ← collapses or goes to
// the parent, Enter opens — all index arithmetic over the rows. Plan-09:
// right-click for create / rename / delete, F2 renames, Delete removes, and
// names are typed in an inline row.
import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { Icon } from "../Icon";
import { validateName, stemLength } from "../files/names";
import type { TreeEdit } from "../files/store";
import { parentIndex, pathKey, type Row } from "../files/tree";

interface Props {
  rows: Row[];
  open: string | null;
  /** Files with unsaved edits, keyed by pathKey. */
  dirty: ReadonlyMap<string, unknown>;
  rootError: string | null;
  /** The inline edit in progress, if it belongs to this project. */
  edit: TreeEdit | null;
  onToggle(path: string): void;
  onOpen(path: string): void;
  /** Resolves false when the operation failed and the row should stay editable. */
  onCommit(name: string): Promise<boolean>;
  onCancelEdit(): void;
  /** `row` is null for the tree's empty space (the root). */
  onContextMenu(row: Row | null, x: number, y: number): void;
  onRename(row: Row): void;
  onDelete(row: Row): void;
  /** The focused row changes; the toolbar's ＋ buttons target its directory. */
  onCursor(row: Row | null): void;
}

const INDENT_PX = 16;

export function FileTree({
  rows, open, dirty, rootError, edit, onToggle, onOpen, onCommit, onCancelEdit, onContextMenu, onRename, onDelete, onCursor,
}: Props) {
  const [cursor, setCursor] = useState(0);
  // The cursor row counts as "focused" only while focus is inside the tree —
  // the row with the focus ring is the one the toolbar's ＋ buttons target.
  const [focusWithin, setFocusWithin] = useState(false);
  const host = useRef<HTMLDivElement>(null);
  const openKey = open ? pathKey(open) : null;

  // Keep the cursor on a real row as the list changes underneath it.
  useEffect(() => {
    if (cursor >= rows.length) setCursor(Math.max(0, rows.length - 1));
  }, [rows.length, cursor]);
  useEffect(() => {
    onCursor(focusWithin ? (rows[cursor] ?? null) : null);
  }, [rows, cursor, focusWithin, onCursor]);

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
      case "F2":
        onRename(row);
        break;
      case "Delete":
        onDelete(row);
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  const onMenu = (e: MouseEvent, row: Row | null) => {
    e.preventDefault();
    e.stopPropagation();
    onContextMenu(row, e.clientX, e.clientY);
  };

  // The create row sits at the top of its directory's children: right after
  // the directory's row, or at the top of the tree for the root.
  const createAfter = edit?.mode === "create" ? pathKey(edit.dir) : null;
  const createAtTop = createAfter !== null && !rows.some((r) => r.kind === "dir" && pathKey(r.path) === createAfter);
  const renameKey = edit?.mode === "rename" ? pathKey(edit.path) : null;

  const createRow = (depth: number) =>
    edit?.mode === "create" && (
      <div key="__create" style={{ paddingLeft: depth * INDENT_PX }}>
        <div className="node ghost" role="treeitem" aria-selected={false}>
          <span className="tw" />
          <Icon name={edit.kind === "dir" ? "folder" : "file"} size={14} />
          <NameInput initial="" onCommit={onCommit} onCancel={onCancelEdit} />
        </div>
      </div>
    );

  return (
    <div
      className="tree"
      ref={host}
      role="tree"
      onContextMenu={(e) => onMenu(e, null)}
      onFocus={() => setFocusWithin(true)}
      onBlur={(e) => setFocusWithin(host.current?.contains(e.relatedTarget as Node | null) ?? false)}
    >
      {rootError && <div className="tree-error">{rootError}</div>}
      {createAtTop && createRow(0)}
      {rows.map((row, i) => {
        const isDir = row.kind === "dir";
        const key = pathKey(row.path);
        const isOpen = !isDir && openKey === key;
        const isDirty = !isDir && dirty.has(key);
        const renaming = renameKey === key;
        const cls = ["node", isDir ? "dir" : "file", isDir && !row.expanded ? "collapsed" : "", isOpen ? "open-file" : ""]
          .filter(Boolean)
          .join(" ");
        // An input cannot live inside a button; a renaming row is a div.
        const RowTag = renaming ? "div" : "button";
        return (
          <div key={row.path}>
            <div style={{ paddingLeft: row.depth * INDENT_PX }}>
              <RowTag
                className={cls}
                role="treeitem"
                aria-expanded={isDir ? row.expanded : undefined}
                aria-selected={isOpen}
                tabIndex={i === cursor ? 0 : -1}
                title={renaming ? undefined : isDirty ? `${row.path} — unsaved changes` : row.path}
                draggable={false}
                onClick={() => {
                  if (renaming) return;
                  setCursor(i);
                  activate(row);
                }}
                onKeyDown={(e) => !renaming && onKey(e, i)}
                onContextMenu={(e) => onMenu(e, row)}
              >
                <span className={`tw${row.expanded && row.state === "loading" ? " busy" : ""}`}>{isDir ? "▼" : ""}</span>
                <Icon name={isDir ? "folder" : "file"} size={14} />
                {renaming ? (
                  <NameInput initial={row.name} onCommit={onCommit} onCancel={onCancelEdit} />
                ) : (
                  <span className="nname">{row.name}</span>
                )}
                {isDirty && !renaming && <span className="dirty" aria-label="unsaved changes" />}
              </RowTag>
              {row.error && row.expanded && <div className="tree-error">{row.error}</div>}
            </div>
            {isDir && row.expanded && createAfter === key && createRow(row.depth + 1)}
          </div>
        );
      })}
    </div>
  );
}

/** The inline name field: Enter commits (when valid), Escape and blur cancel. */
function NameInput({ initial, onCommit, onCancel }: { initial: string; onCommit(name: string): Promise<boolean>; onCancel(): void }) {
  const [value, setValue] = useState(initial);
  const ref = useRef<HTMLInputElement>(null);
  const error = validateName(value);
  // Committing blurs the input; the blur must not then cancel.
  const done = useRef(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(0, initial ? stemLength(initial) : 0);
  }, [initial]);

  return (
    <input
      ref={ref}
      className="rename"
      value={value}
      spellCheck={false}
      aria-invalid={error !== null}
      title={error ?? undefined}
      onChange={(e) => setValue(e.target.value)}
      onClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") {
          e.preventDefault();
          if (error === null) {
            done.current = true;
            void onCommit(value).then((ok) => {
              if (!ok) {
                // Stays editable: the toast said why. Blur cancels again from here.
                done.current = false;
                ref.current?.focus();
              }
            });
          }
        } else if (e.key === "Escape") {
          e.preventDefault();
          done.current = true;
          onCancel();
        }
      }}
      onBlur={() => {
        if (!done.current) onCancel();
      }}
    />
  );
}
