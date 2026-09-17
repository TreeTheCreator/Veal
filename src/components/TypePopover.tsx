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
  /** The `data-popover-anchor` value of the trigger that opened this popover; its own pointerdowns are ignored. */
  anchor: string;
  className?: string;
}

export function TypePopover({ cwdLabel, defaultKind, onPick, onClose, anchor, className = "" }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const shellRef = useRef<HTMLButtonElement>(null);
  const agentRef = useRef<HTMLButtonElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const defaultRef = useRef(defaultKind);

  // Runs once on open: focus the default option, close on outside click / Escape.
  useEffect(() => {
    (defaultRef.current === "agent" ? agentRef : shellRef).current?.focus();
    // A pointerdown on the trigger that opened us is its business (plan-13):
    // closing here and letting its click reopen made the trigger a no-op.
    const onDown = (e: PointerEvent) => {
      const target = e.target as Element | null;
      const onAnchor = target?.closest?.("[data-popover-anchor]")?.getAttribute("data-popover-anchor") === anchor;
      if (ref.current?.contains(target) || onAnchor) return;
      onCloseRef.current();
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
  }, [anchor]);

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
          <small>Label only — the badge follows what actually runs</small>
        </span>
      </button>
    </div>
  );
}
