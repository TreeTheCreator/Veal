// Step 8: the shell with sidebar, view switching, persistence, Files (with editing), Tasks and file watching.
// Boot loads settings + projects from SQLite, restores the pane layout (or
// opens one shell in the active project), then starts writing layout changes
// back. All views stay mounted; inactive ones are hidden with `visibility`
// so the terminal container keeps its size. Shells outlive the page: a reload
// finds them running and reattaches.
import { useCallback, useEffect, useState } from "react";
import { homeDir } from "@tauri-apps/api/path";
import { Logo } from "./Icon";
import { AddProjectModal } from "./components/AddProjectModal";
import { CloseGuard } from "./components/CloseGuard";
import { DaemonMismatch } from "./components/DaemonMismatch";
import { FilesView } from "./components/FilesView";
import { NewTerminalButton } from "./components/NewTerminalButton";
import { PaneLayout } from "./components/PaneLayout";
import { SettingsView } from "./components/SettingsView";
import { Sidebar } from "./components/Sidebar";
import { TasksView } from "./components/TasksView";
import { Toast } from "./components/Toast";
import { UpdateCard } from "./components/UpdateCard";
import { parseAllFiles, startFilesPersistence } from "./files/persist";
import { useFiles } from "./files/store";
import { LAYOUT_KEY, parseLayout, serializeLayout, startLayoutPersistence } from "./layout/persist";
import { reconcile, startSessionKiller } from "./layout/sessions";
import { selectPaneCount, useLayout } from "./layout/store";
import { PANE_CAP, panes } from "./layout/tree";
import { setHome, tilde } from "./paths";
import { selectActive, useProjects } from "./projects/store";
import { db } from "./db";
import { forgetSession, killSession, listSessions, parseColor, setTheme } from "./pty";
import { useSettings } from "./settings/store";
import { terminalTheme, useEffectiveMode } from "./theme";
import { isView, TAB_LABEL, TAB_VIEWS, useUi, VIEW_KEY, viewForShortcut } from "./ui";
import { startWatching } from "./watch";


// StrictMode runs effects twice in dev; the restore must happen once.
let bootStarted = false;

async function boot() {
  const [settings, sessions] = await Promise.all([
    useSettings.getState().load(),
    // Shells the page before this one (F5, "Refresh") left running.
    listSessions().catch((e) => {
      console.warn("pty_list", e);
      return [];
    }),
    useProjects.getState().load(),
    // Paths under the profile display as `~`; shown whole until this resolves.
    homeDir().then(setHome, (e) => console.warn("homeDir", e)),
  ]);
  const view = settings[VIEW_KEY];
  if (isView(view)) useUi.setState(view === "settings" ? { view } : { view, lastTab: view });
  const restoreOn = useSettings.getState().restoreLayout;
  const restored = restoreOn ? parseLayout(settings[LAYOUT_KEY]) : null;
  if (restoreOn) useFiles.getState().restore(parseAllFiles(settings));
  const { tree, nextId, forget } = reconcile(restored?.tree ?? null, restored?.nextId ?? 1, sessions);
  for (const key of forget) forgetSession(key).catch((e) => console.warn("pty_forget", e));
  const leaves = panes(tree);
  const focused = restored && leaves.some((p) => p.id === restored.focused) ? restored.focused : (leaves[0]?.id ?? null);
  useLayout.setState({ tree, focused, nextId });
  // From here on, a pane leaving the layout ends its shell. Installed after the
  // restore, though the restore removes nothing it should act on anyway.
  startSessionKiller((key) => killSession(key).catch((e) => console.warn("pty_kill", e)));
  if (!tree) {
    const active = selectActive(useProjects.getState());
    if (active) useLayout.getState().addPane(active.path);
  }
  // Written now, not on the next change: a pane given a new key by this restore
  // would otherwise get yet another one on the next reload, and its shell would
  // come back as an extra pane.
  await db
    .settingsSet(LAYOUT_KEY, serializeLayout(useLayout.getState()))
    .catch((e) => console.warn("layout save", e));
  startLayoutPersistence();
  startFilesPersistence();
  startWatching();
}

export default function App() {
  const count = useLayout(selectPaneCount);
  const addPane = useLayout((s) => s.addPane);
  const active = useProjects(selectActive);
  const loaded = useProjects((s) => s.loaded);
  const view = useUi((s) => s.view);
  const setView = useUi((s) => s.setView);
  const [ready, setReady] = useState(false);
  const [adding, setAdding] = useState(false);
  const closeAdd = useCallback(() => setAdding(false), []);
  const mode = useEffectiveMode();

  // The daemon answers OSC 10/11 for shells with no window attached, from the
  // colours reported here (plan-19 5.10). Once, not per pane.
  useEffect(() => {
    const t = terminalTheme();
    const fg = parseColor(t.foreground);
    const bg = parseColor(t.background);
    if (fg && bg) setTheme(fg, bg).catch((e) => console.warn("pty_theme", e));
  }, [mode]);

  useEffect(() => {
    if (bootStarted) return;
    bootStarted = true;
    boot()
      .catch((e) => console.error("boot", e))
      .finally(() => setReady(true));
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const v = viewForShortcut(e);
      if (v) {
        e.preventDefault();
        setView(v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setView]);

  const atCap = count >= PANE_CAP;
  const cwdLabel = active ? tilde(active.path) : null;
  const hint = !cwdLabel
    ? loaded
      ? "Pin a project to open a terminal"
      : ""
    : atCap
      ? `Soft cap of ${PANE_CAP} reached`
      : `Opens in ${cwdLabel}`;

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand"><Logo size={20} />veal <small>0.1</small></div>
        <div className="topbar-right"><span className="mono">{count}/{PANE_CAP} panes</span></div>
        <div className="tabs" role="tablist">
          {TAB_VIEWS.map((v, i) => (
            <button
              key={v}
              className={`tab${view === v ? " active" : ""}`}
              role="tab"
              aria-selected={view === v}
              onClick={() => setView(v)}
            >
              {TAB_LABEL[v]} <kbd>⌃{i + 1}</kbd>
            </button>
          ))}
        </div>
      </header>

      <Sidebar onAddProject={() => setAdding(true)} />

      <main className="main">
        <div className="views">
          <section className={`view${view === "terminal" ? " active" : ""}`} aria-hidden={view !== "terminal"}>
            <div className="toolbar">
              <NewTerminalButton
                disabled={!active || atCap}
                onNew={() => active && addPane(active.path)}
              />
              <span className="hint">{hint}</span>
              <span className="grow" />
              <span className="hint">Drag the dividers to resize</span>
            </div>
            {/* Mount the layout only once the restore has run, so the seed pane is never raced. */}
            {ready && (
              <PaneLayout
                emptyHint={cwdLabel ? `New terminal spawns a PTY in ${cwdLabel}` : "Pin a project to open a terminal"}
              />
            )}
          </section>

          <section className={`view${view === "files" ? " active" : ""}`} aria-hidden={view !== "files"}>
            <FilesView visible={view === "files"} />
          </section>

          <section className={`view${view === "tasks" ? " active" : ""}`} aria-hidden={view !== "tasks"}>
            <TasksView visible={view === "tasks"} />
          </section>

          <section className={`view settings-view${view === "settings" ? " active" : ""}`} aria-hidden={view !== "settings"}>
            <SettingsView />
          </section>
        </div>
      </main>

      {adding && <AddProjectModal onClose={closeAdd} />}
      <CloseGuard />
      <DaemonMismatch />
      <Toast />
      <UpdateCard />
    </div>
  );
}
