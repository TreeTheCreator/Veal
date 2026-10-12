// Update prompt. Checks the release feed once, a few seconds after launch;
// offline or a missing feed is logged, not shown. Installing exits Veal (the
// NSIS installer runs passively and relaunches it), so it warns first about
// what the close guard would: unsaved edits and running agents.
import { check, type Update } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { useFiles } from "../files/store";
import { useLayout } from "../layout/store";
import { runningAgents, useProjects } from "../projects/store";
import { flushAll } from "./CloseGuard";

const CHECK_DELAY_MS = 5000;

type Phase = { kind: "offer" } | { kind: "confirm"; warning: string } | { kind: "busy"; label: string } | { kind: "error"; message: string };

function plural(n: number, word: string) {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

/** What installing would lose, or null when nothing is at stake. */
function atStake(): string | null {
  const dirty = useFiles.getState().dirtyFiles().length;
  const { tree, agents } = useLayout.getState();
  const running = runningAgents(tree, useProjects.getState().projects, agents).length;
  const parts = [dirty && `${plural(dirty, "file")} with unsaved edits`, running && `${plural(running, "running agent")}`].filter(Boolean);
  return parts.length ? `Updating closes Veal: ${parts.join(" and ")} will be lost.` : null;
}

export function UpdateCard() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [phase, setPhase] = useState<Phase>({ kind: "offer" });

  useEffect(() => {
    if (import.meta.env.DEV) return;
    const t = setTimeout(() => {
      check()
        .then((u) => u && setUpdate(u))
        .catch((e) => console.warn("update check", e));
    }, CHECK_DELAY_MS);
    return () => clearTimeout(t);
  }, []);

  if (!update) return null;

  const install = async () => {
    let total = 0;
    let got = 0;
    setPhase({ kind: "busy", label: "Downloading…" });
    try {
      await update.download((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        if (e.event === "Progress" && total) {
          got += e.data.chunkLength;
          setPhase({ kind: "busy", label: `Downloading… ${Math.round((got / total) * 100)}%` });
        }
      });
      setPhase({ kind: "busy", label: "Installing…" });
      await flushAll();
      await update.install();
    } catch (e) {
      console.error("update install", e);
      setPhase({ kind: "error", message: String(e) });
    }
  };

  const start = () => {
    const warning = atStake();
    if (warning) setPhase({ kind: "confirm", warning });
    else install();
  };

  return (
    <div className="update-card" role="status">
      <b>Veal {update.version} is available</b>
      <span>
        {phase.kind === "offer" && `You have ${update.currentVersion}. Veal restarts to install it.`}
        {phase.kind === "confirm" && phase.warning}
        {phase.kind === "busy" && phase.label}
        {phase.kind === "error" && `Update failed: ${phase.message}`}
      </span>
      {phase.kind !== "busy" && (
        <div className="row">
          <button className="btn ghost" onClick={() => setUpdate(null)}>Later</button>
          <button className="btn primary" onClick={phase.kind === "confirm" ? install : start}>
            {phase.kind === "confirm" ? "Update anyway" : phase.kind === "error" ? "Try again" : "Restart to update"}
          </button>
        </div>
      )}
    </div>
  );
}
