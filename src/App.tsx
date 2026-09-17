// Step 2: the static shell from step 1 plus one real PTY pane filling the
// content area. Tab switching, sidebar state and pane splits arrive later.
import { Icon } from "./Icon";
import { TerminalPane } from "./components/TerminalPane";

// Hard-coded for this step; the sidebar supplies it from step 4.
const CWD = String.raw`C:\Users\user\veal`;

export default function App() {
  return (
    <div className="app">
      <header className="topbar">
        <div className="brand"><Icon name="term" size={20} />veal <small>0.1</small></div>
        <div className="tabs" role="tablist">
          <button className="tab active" role="tab">Terminal <kbd>⌃1</kbd></button>
          <button className="tab" role="tab">Files <kbd>⌃2</kbd></button>
          <button className="tab" role="tab">Tasks <kbd>⌃3</kbd></button>
        </div>
        <div className="topbar-right"><span className="pill">step 2</span></div>
      </header>

      <aside className="sidebar">
        <div className="side-head"><span>Pinned projects</span><span className="count">0</span></div>
        <ul className="projects" />
        <button className="add-project"><span className="plus">+</span>Add project</button>
        <div className="side-foot"><button className="settings-btn"><Icon name="gear" />Settings</button></div>
      </aside>

      <main className="main">
        <div className="toolbar"><span className="hint mono">{CWD}</span></div>
        <TerminalPane cwd={CWD} />
      </main>
    </div>
  );
}
