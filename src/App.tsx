// Step 3: the static shell plus a resizable split tree of PTY panes in the
// content area. Tab switching and sidebar state arrive in step 4.
import { useEffect } from "react";
import { Icon } from "./Icon";
import { NewTerminalButton } from "./components/NewTerminalButton";
import { PaneLayout } from "./components/PaneLayout";
import { selectPaneCount, useLayout } from "./layout/store";
import { PANE_CAP } from "./layout/tree";

// Hard-coded for this step; the sidebar supplies it from step 4.
const CWD = String.raw`C:\Users\user\veal`;
const CWD_LABEL = "~\\veal";

export default function App() {
  const count = useLayout(selectPaneCount);
  const addPane = useLayout((s) => s.addPane);
  const atCap = count >= PANE_CAP;

  // Open straight into a prompt, like step 2.
  useEffect(() => {
    if (useLayout.getState().tree === null) addPane("shell", CWD);
  }, [addPane]);

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand"><Icon name="term" size={20} />veal <small>0.1</small></div>
        <div className="tabs" role="tablist">
          <button className="tab active" role="tab">Terminal <kbd>⌃1</kbd></button>
          <button className="tab" role="tab">Files <kbd>⌃2</kbd></button>
          <button className="tab" role="tab">Tasks <kbd>⌃3</kbd></button>
        </div>
        <div className="topbar-right"><span className="mono">{count}/{PANE_CAP} panes</span></div>
      </header>

      <aside className="sidebar">
        <div className="side-head"><span>Pinned projects</span><span className="count">0</span></div>
        <ul className="projects" />
        <button className="add-project"><span className="plus">+</span>Add project</button>
        <div className="side-foot"><button className="settings-btn"><Icon name="gear" />Settings</button></div>
      </aside>

      <main className="main">
        <div className="toolbar">
          <NewTerminalButton disabled={atCap} cwdLabel={CWD_LABEL} onPick={(kind) => addPane(kind, CWD)} />
          <span className="hint">{atCap ? `Soft cap of ${PANE_CAP} reached` : `Opens in ${CWD_LABEL}`}</span>
          <span className="grow" />
          <span className="hint">Drag the dividers to resize · click a badge to flip agent ⇄ shell</span>
        </div>
        <PaneLayout emptyHint={`New terminal spawns a PTY in ${CWD_LABEL}`} />
      </main>
    </div>
  );
}
