// "+ New terminal" with the mockup's type popover (Shell / Agent).
import { useEffect, useRef, useState } from "react";
import type { PaneKind } from "../layout/tree";

interface Props {
  disabled: boolean;
  cwdLabel: string;
  onPick(kind: PaneKind): void;
}

export function NewTerminalButton({ disabled, cwdLabel, onPick }: Props) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);
  const firstRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    firstRef.current?.focus();
    const onDown = (e: PointerEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const pick = (kind: PaneKind) => {
    setOpen(false);
    onPick(kind);
  };

  return (
    <div className="new-terminal" ref={wrapRef}>
      <button className="btn primary" disabled={disabled} onClick={() => setOpen((o) => !o)}>
        + New terminal
      </button>
      {open && (
        <div className="popover open" role="menu">
          <div className="ttl">
            New terminal in <span className="mono">{cwdLabel}</span>
          </div>
          <button className="opt" role="menuitem" ref={firstRef} onClick={() => pick("shell")}>
            <span className="sw" style={{ background: "var(--shell)" }} />
            <span>
              <b>Shell</b>
              <small>pwsh in the project root</small>
            </span>
          </button>
          <button className="opt" role="menuitem" onClick={() => pick("agent")}>
            <span className="sw" style={{ background: "var(--agent)" }} />
            <span>
              <b>Agent</b>
              <small>Same PTY — you launch claude, aider, etc.</small>
            </span>
          </button>
        </div>
      )}
    </div>
  );
}
