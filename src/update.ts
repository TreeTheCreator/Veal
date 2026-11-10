// Installing an update (plan-19 7.4). The installer replaces veal-daemon.exe,
// which it cannot do while the daemon runs, and replacing it ends its shells
// anyway: so after the download, save, then stop the background service and
// wait for it to go, then install. A stop that times out is logged and the
// install goes ahead; the installer's own hook is the backstop.
import type { Update } from "@tauri-apps/plugin-updater";
import { flushAll } from "./components/CloseGuard";
import { setCloseMode, shutdownDaemon } from "./pty";

export async function installUpdate(update: Pick<Update, "download" | "install">, progress: (label: string) => void): Promise<void> {
  let total = 0;
  let got = 0;
  progress("Downloading…");
  await update.download((e) => {
    if (e.event === "Started") total = e.data.contentLength ?? 0;
    if (e.event === "Progress" && total) {
      got += e.data.chunkLength;
      progress(`Downloading… ${Math.round((got / total) * 100)}%`);
    }
  });
  progress("Installing…");
  await flushAll();
  await setCloseMode(false).catch((e) => console.warn("pty_close_mode", e));
  const gone = await shutdownDaemon().catch((e) => {
    console.warn("pty_shutdown", e);
    return false;
  });
  if (!gone) console.warn("background service still running; the installer will stop it");
  await update.install();
}
