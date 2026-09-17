// Filename → CodeMirror language. `language-data` matches by extension and
// filename and lazy-loads the language chunk; the alias table covers the few
// names it misses that this machine hits. Pure; vitest-tested.
import { LanguageDescription } from "@codemirror/language";
import { languages } from "@codemirror/language-data";

const ALIASES: Record<string, string> = {
  ".mjs": "JavaScript",
  ".cjs": "JavaScript",
  ".tsx": "TSX",
  ".ps1": "PowerShell",
  ".psm1": "PowerShell",
  "cargo.lock": "TOML",
  "pnpm-lock.yaml": "YAML",
};

export function languageFor(name: string): LanguageDescription | null {
  const lower = name.toLowerCase();
  const ext = lower.includes(".") ? lower.slice(lower.lastIndexOf(".")) : "";
  const alias = ALIASES[lower] ?? ALIASES[ext];
  if (alias) return LanguageDescription.matchLanguageName(languages, alias, true);
  // Dotfiles like `.gitignore` have no extension; matchFilename would see ".gitignore" as one.
  if (lower.startsWith(".") && !lower.slice(1).includes(".")) return null;
  return LanguageDescription.matchFilename(languages, name);
}
