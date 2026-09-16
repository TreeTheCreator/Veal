// Step 1: the static three-view shell from the mockup. No state yet — this only
// proves Vite + React + Tauri are wired together. Views arrive in later steps.
import { Icon } from "./Icon";

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
        <div className="topbar-right"><span className="pill">step 1</span></div>
      </header>

      <aside className="sidebar">
        <div className="side-head"><span>Pinned projects</span><span className="count">0</span></div>
        <ul className="projects" />
        <button className="add-project"><span className="plus">+</span>Add project</button>
        <div className="side-foot"><button className="settings-btn"><Icon name="gear" />Settings</button></div>
      </aside>

      <main className="main">
        <div className="toolbar"><span className="hint">Terminal view — PTY panes arrive in step 2</span></div>
        <div className="empty">No panes yet</div>
      </main>
    </div>
  );
}
