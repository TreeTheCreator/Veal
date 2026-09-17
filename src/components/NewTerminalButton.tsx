// "+ New terminal" with the type popover (Shell / Agent).
import { useCallback, useState } from "react";
import type { PaneKind } from "../layout/tree";
import { useSettings } from "../settings/store";
import { TypePopover } from "./TypePopover";

interface Props {
  disabled: boolean;
  cwdLabel: string;
  onPick(kind: PaneKind): void;
}

export function NewTerminalButton({ disabled, cwdLabel, onPick }: Props) {
  const [open, setOpen] = useState(false);
  const defaultKind = useSettings((s) => s.defaultKind);
  const close = useCallback(() => setOpen(false), []);

  return (
    <div className="new-terminal">
      <button className="btn primary" disabled={disabled} data-popover-anchor="new-terminal" onClick={() => setOpen((o) => !o)}>
        + New terminal
      </button>
      {open && (
        <TypePopover
          cwdLabel={cwdLabel}
          defaultKind={defaultKind}
          anchor="new-terminal"
          onClose={close}
          onPick={(kind) => {
            setOpen(false);
            onPick(kind);
          }}
        />
      )}
    </div>
  );
}
