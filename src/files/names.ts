// Name rules for create / rename (plan-09, decision 3): Windows-strict on
// every platform, so a name made here can be checked out anywhere. Mirrors
// `validate_name` in src-tauri/src/fs.rs, which still decides; this is the
// inline feedback while typing. Pure; vitest-tested.

const RESERVED = new Set([
  "con", "prn", "aux", "nul",
  "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
  "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
]);

/** The problem with `name`, or null when it is fine. */
export function validateName(name: string): string | null {
  if (name.trim() === "") return "name is empty";
  if (name !== name.trim()) return "no leading or trailing spaces";
  if (name === "." || name === "..") return "not a valid name";
  if (/[\\/]/.test(name)) return "no slashes in a name";
  const bad = name.match(/[<>:"|?*]/);
  if (bad) return `${bad[0]} is not allowed in a name`;
  // eslint-disable-next-line no-control-regex
  if (/[\x00-\x1f\x7f]/.test(name)) return "no control characters";
  if (name.endsWith(".")) return "a name cannot end with a dot";
  if (new TextEncoder().encode(name).length > 255) return "name is too long";
  const stem = name.split(".")[0].toLowerCase();
  if (RESERVED.has(stem)) return `${stem.toUpperCase()} is a reserved name on Windows`;
  return null;
}

/** `App.tsx` → 3 (the stem's length), so a rename can select just the stem; `.env` → 4. */
export function stemLength(name: string): number {
  const i = name.lastIndexOf(".");
  return i <= 0 ? name.length : i;
}
