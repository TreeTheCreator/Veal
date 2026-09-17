// The mockup's "new terminal" type popover (Shell / Agent). Anchored by its
// parent (`position: relative`); closes on outside click or Escape.
import { useEffect, useRef } from "react";
import type { PaneKind } from "../layout/tree";

interface Props {
  cwdLabel: string;
  /** Receives keyboard focus when the popover opens. */
  defaultKind: PaneKind;
  onPick(kind: PaneKind): void;
  onClose(): void;
  className?: string;
}

export function TypePopover({ cwdLabel, defaultKind, onPick, onClose, className = "" }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const shellRef = useRef<HTMLButtonElement>(null);
  const agentRef = useRef<HTMLButtonElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const defaultRef = useRef(defaultKind);

  // Runs once on open: focus the default option, close on outside click / Escape.
  useEffect(() => {
    (defaultRef.current === "agent" ? agentRef : shellRef).current?.focus();
    const onDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onCloseRef.current();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCloseRef.current();
    };
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, []);

  return (
    <div className={`popover open ${className}`} role="menu" ref={ref}>
      <div className="ttl">
        New terminal in <span className="mono">{cwdLabel}</span>
      </div>
      <button className="opt" role="menuitem" ref={shellRef} onClick={() => onPick("shell")}>
        <span className="sw" style={{ background: "var(--shell)" }} />
        <span>
          <b>Shell</b>
          <small>pwsh in the project root</small>
        </span>
      </button>
      <button className="opt" role="menuitem" ref={agentRef} onClick={() => onPick("agent")}>
        <span className="sw" style={{ background: "var(--agent)" }} />
        <span>
          <b>Agent</b>
          <small>Same PTY — you launch claude, aider, etc.</small>
        </span>
      </button>
    </div>
  );
}
