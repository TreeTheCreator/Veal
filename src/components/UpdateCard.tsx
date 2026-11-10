// Update prompt. Checks the release feed once, a few seconds after launch;
// offline or a missing feed is logged, not shown. Installing exits Veal (the
// NSIS installer runs passively and relaunches it) and stops the background
// service (plan-19 7.4), so it warns first about unsaved edits and running
// agents.
import { check, type Update } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { useFiles } from "../files/store";
import { useLayout } from "../layout/store";
import { runningAgents, useProjects } from "../projects/store";
import { installUpdate } from "../update";

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
  if (!dirty && !running) return null;
  const parts = [];
  if (running) parts.push(`Updating stops Veal's background service: ${plural(running, "running agent")} will end.`);
  if (dirty) parts.push(`${plural(dirty, "file")} with unsaved edits will be lost.`);
  return parts.join(" ");
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
    try {
      await installUpdate(update, (label) => setPhase({ kind: "busy", label }));
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
