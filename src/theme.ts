// Plan-11: the effective colour mode. CSS does the work for the app (tokens
// under `data-theme` / `prefers-color-scheme`); xterm needs concrete colours,
// so it reads the tokens from computed style whenever the mode changes.
import { useEffect, useState } from "react";
import { useSettings, type Theme } from "./settings/store";
export { applyTheme } from "./settings/store";

export type Mode = "dark" | "light";

const LIGHT_QUERY = "(prefers-color-scheme: light)";

export function effectiveMode(theme: Theme, systemLight: boolean = window.matchMedia(LIGHT_QUERY).matches): Mode {
  if (theme === "system") return systemLight ? "light" : "dark";
  return theme;
}

/** Re-renders on the setting or the OS preference changing. */
export function useEffectiveMode(): Mode {
  const theme = useSettings((s) => s.theme);
  const [systemLight, setSystemLight] = useState(() => window.matchMedia(LIGHT_QUERY).matches);
  useEffect(() => {
    const mq = window.matchMedia(LIGHT_QUERY);
    const onChange = (e: MediaQueryListEvent) => setSystemLight(e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return effectiveMode(theme, systemLight);
}

/** xterm's palette from the live tokens; called after the mode has been applied. */
export function terminalTheme() {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string) => css.getPropertyValue(name).trim();
  return {
    background: v("--bg-sunk"),
    foreground: v("--text"),
    cursor: v("--accent"),
    selectionBackground: v("--sel"),
  };
}
