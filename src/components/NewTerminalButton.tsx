// "+ New terminal": opens a shell pane in the active project at once
// (plan-17). An agent is detected when one runs, not picked here.
interface Props {
  disabled: boolean;
  onNew(): void;
}

export function NewTerminalButton({ disabled, onNew }: Props) {
  return (
    <div className="new-terminal">
      <button className="btn primary" disabled={disabled} onClick={onNew}>
        + New terminal
      </button>
    </div>
  );
}
