// What closing the window does (plan-19 section 8). Unsaved files always ask;
// with no agents running, closing ends the idle shells; otherwise the saved
// choice decides, and `ask` shows the dialog (E5).
import type { CloseWithAgents } from "./settings/store";

export type CloseDecision = "ask" | "keep" | "stop";

export function closeDecision(dirty: number, agents: number, setting: CloseWithAgents): CloseDecision {
  if (dirty > 0) return "ask";
  if (agents === 0) return "stop";
  return setting;
}
