// A background service of another version is running sessions (plan-19 7.5).
// Installs normally stay in step, since an update stops the service first, so
// this means something went wrong. Restart ends those sessions and starts this
// version's service; Quit leaves them alone.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { daemonMismatch, onDaemonMismatch, replaceDaemon } from "../pty";
import { flushAll } from "./CloseGuard";

function plural(n: number, word: string) {
  return `${n} ${word}${n === 1 ? " is" : "s are"}`;
}

export function DaemonMismatch({ reload = () => location.reload() }: { reload?: () => void }) {
  const [sessions, setSessions] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    daemonMismatch()
      .then((n) => n !== null && setSessions((s) => s ?? n))
      .catch((e) => console.warn("pty_mismatch", e));
    const off = onDaemonMismatch((n) => setSessions(n));
    return () => {
      off.then((f) => f()).catch(() => {});
    };
  }, []);

  if (sessions === null) return null;

  const restart = () => {
    setBusy(true);
    setError(null);
    replaceDaemon()
      // Boot reconciles against the new service, as after F5: the panes
      // respawn in their saved folders.
      .then(reload)
      .catch((e) => {
        setBusy(false);
        setError(String(e));
      });
  };
  const quit = () => {
    flushAll()
      .then(() => getCurrentWindow().destroy())
      .catch((e) => console.error("destroy", e));
  };

  return (
    <div className="scrim open">
      <div className="modal" role="dialog" aria-modal="true">
        <h3>Older background service running</h3>
        <div className="field">
          <span>
            {plural(sessions, "session")} running on an older version of Veal's background service. Restart ends them and starts this
            version's.
          </span>
          {error && <small className="error">{error}</small>}
        </div>
        <div className="row">
          <button type="button" className="btn ghost" onClick={quit} disabled={busy}>
            Quit
          </button>
          <button type="button" className="btn primary" onClick={restart} disabled={busy} autoFocus>
            {busy ? "Restarting…" : "Restart"}
          </button>
        </div>
      </div>
    </div>
  );
}
