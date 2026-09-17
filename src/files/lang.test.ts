import { describe, expect, it } from "vitest";
import { languageFor } from "./lang";

describe("languageFor", () => {
  it.each([
    ["App.tsx", "TSX"],
    ["store.ts", "TypeScript"],
    ["vite.config.mjs", "JavaScript"],
    ["db.rs", "Rust"],
    ["Cargo.toml", "TOML"],
    ["Cargo.lock", "TOML"],
    ["tasks.json", "JSON"],
    ["README.md", "Markdown"],
    ["pnpm-lock.yaml", "YAML"],
    ["index.html", "HTML"],
    ["styles.css", "CSS"],
    ["build.ps1", "PowerShell"],
    ["main.py", "Python"],
  ])("%s → %s", (name, lang) => {
    expect(languageFor(name)?.name).toBe(lang);
  });

  it.each([".gitignore", "LICENSE", "notes.txt"])("%s → plain text", (name) => {
    expect(languageFor(name)).toBeNull();
  });
});
