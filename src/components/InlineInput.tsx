// One inline text field for renaming things in place (plan-09 tree names,
// plan-10 task titles): Enter commits when `validate` is happy, Escape and
// blur cancel. A failed commit (Rust said no) keeps the field open with
// its text; the toast said why.
import { useEffect, useRef, useState } from "react";

interface Props {
  initial: string;
  /** The problem with `value`, or null when it may be committed. */
  validate(value: string): string | null;
  /** How much of `initial` to select on mount (default: all of it). */
  selectTo?(initial: string): number;
  /** Resolves false when the operation failed and the field should stay open. */
  onCommit(value: string): Promise<boolean>;
  onCancel(): void;
  className?: string;
}

export function InlineInput({ initial, validate, selectTo, onCommit, onCancel, className = "" }: Props) {
  const [value, setValue] = useState(initial);
  const ref = useRef<HTMLInputElement>(null);
  const error = validate(value);
  // Committing blurs the input; the blur must not then cancel.
  const done = useRef(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(0, selectTo ? selectTo(initial) : initial.length);
  }, [initial, selectTo]);

  return (
    <input
      ref={ref}
      className={className}
      value={value}
      spellCheck={false}
      aria-invalid={error !== null}
      title={error ?? undefined}
      onChange={(e) => setValue(e.target.value)}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") {
          e.preventDefault();
          if (error === null) {
            done.current = true;
            void onCommit(value).then((ok) => {
              if (!ok) {
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
