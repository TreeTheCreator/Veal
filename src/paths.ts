// Path helpers shared by the sidebar, pane headers and the add-project modal.
// Pure and Windows-aware (case-insensitive, either separator).

const SEPS = /[\\/]+/;

export function basename(path: string): string {
  const parts = path.split(SEPS).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

let home = "";

/** The profile directory `tilde` folds; `App.boot` sets it from Tauri's `homeDir()`. */
export function setHome(dir: string): void {
  home = normalizePath(dir);
}

/**
 * `C:\Users\user\veal` → `~\veal`, for display. Only the known home is
 * folded (plan-14): another profile, `Public`, or anything before
 * `setHome` has run, is shown whole.
 */
export function tilde(path: string): string {
  if (!home) return path;
  const p = normalizePath(path).replace(/\//g, "\\");
  const h = home.replace(/\//g, "\\");
  if (p.toLowerCase() === h.toLowerCase()) return "~";
  const prefix = `${h}\\`;
  return p.toLowerCase().startsWith(prefix.toLowerCase()) ? `~\\${p.slice(prefix.length)}` : path;
}

/**
 * Strips trailing separators and, for a drive-letter path, turns `/` into `\`
 * so a typed `C:/x/` pins as `C:\x`. A bare drive root (`C:\`) stays intact.
 */
export function normalizePath(path: string): string {
  let trimmed = path.trim().replace(/[\\/]+$/, "");
  if (/^[A-Za-z]:/.test(trimmed)) trimmed = trimmed.replace(/\//g, "\\");
  return /^[A-Za-z]:$/.test(trimmed) ? `${trimmed}\\` : trimmed;
}

/** `C:\Users\user\veal\` and `c:/users/user/veal` are the same directory. */
export function samePath(a: string, b: string): boolean {
  const key = (p: string) => normalizePath(p).replace(/\//g, "\\").toLowerCase();
  return key(a) === key(b);
}

export function joinPath(dir: string, name: string): string {
  const sep = dir.includes("/") && !dir.includes("\\") ? "/" : "\\";
  return `${normalizePath(dir)}${sep}${name}`;
}

/** A drive-letter, UNC or POSIX-rooted path; anything else is relative. */
export function isAbsolutePath(path: string): boolean {
  return /^[A-Za-z]:/.test(path) || /^[\\/]/.test(path);
}

export interface ProjectDraft {
  name: string;
  path: string;
  /** True when the path was resolved under `home` (a blank or relative root). */
  created: boolean;
}

/**
 * The add-project modal's derivations (from the mockup): blank root → `~\name`,
 * blank name → basename(root), both blank → nothing to submit. A relative
 * root goes under home too (plan-16): Rust refuses anything relative, so it
 * must never reach it as typed — the exe's cwd is not a place.
 */
export function deriveProject(name: string, root: string, home: string): ProjectDraft | null {
  const n = name.trim();
  const r = normalizePath(root);
  if (!n && !r) return null;
  if (!r) return { name: n, path: joinPath(home, n), created: true };
  if (!isAbsolutePath(r)) return { name: n || basename(r), path: joinPath(home, r), created: true };
  return { name: n || basename(r), path: r, created: false };
}
