// Pinned projects (mockup `renderSidebar`): click selects, hover `+` opens a
// shell pane in that project (plan-17), the dot shows a live agent pane, right-click
// unpins. The Settings entry is pinned to the foot.
import { useEffect, useMemo, useState, type MouseEvent } from "react";
import { Icon } from "../Icon";
import type { Project } from "../db";
import { useFiles, type DirtyFile } from "../files/store";
import { useLayout } from "../layout/store";
import { PANE_CAP, panes } from "../layout/tree";
import { tilde } from "../paths";
import { clampMenu } from "../menu";
import { liveProjectIds, useProjects } from "../projects/store";
import { useUi } from "../ui";
import { relativeTo } from "./FilesView";

interface Props {
  onAddProject(): void;
}

const MISSING_TITLE = "Folder not found — restore it or unpin";

interface Menu {
  x: number;
  y: number;
  project: Project;
}

/** An unpin held back by unsaved edits (plan-12); the modal decides. */
interface PendingUnpin {
  project: Project;
  dirty: DirtyFile[];
}

export function Sidebar({ onAddProject }: Props) {
  const projects = useProjects((s) => s.projects);
  const activeId = useProjects((s) => s.activeId);
  const setActive = useProjects((s) => s.setActive);
  const remove = useProjects((s) => s.remove);
  const tree = useLayout((s) => s.tree);
  const agents = useLayout((s) => s.agents);
  const addPane = useLayout((s) => s.addPane);
  const view = useUi((s) => s.view);
  const setView = useUi((s) => s.setView);
  const toggleSettings = useUi((s) => s.toggleSettings);
  const showToast = useUi((s) => s.showToast);

  const live = useMemo(() => liveProjectIds(tree, projects, agents), [tree, projects, agents]);
  const atCap = panes(tree).length >= PANE_CAP;

  const [menu, setMenu] = useState<Menu | null>(null);
  const [pending, setPending] = useState<PendingUnpin | null>(null);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  const spawn = (project: Project) => {
    if (view !== "terminal") setView("terminal");
    if (addPane(project.path) === null) showToast(`Soft cap of ${PANE_CAP} reached`);
  };

  useEffect(() => {
    if (!pending) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setPending(null);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [pending]);

  const unpin = async (project: Project) => {
    setPending(null);
    try {
      await remove(project.id);
      showToast(`Unpinned ${project.name}`);
    } catch (e) {
      showToast(`Could not unpin: ${String(e)}`);
    }
  };

  // Unpinning forgets the project's drafts, so unsaved edits get the same
  // gate as deleting a file or quitting: a modal, never a silent drop.
  const requestUnpin = (project: Project) => {
    setMenu(null);
    const dirty = useFiles.getState().dirtyFiles().filter((d) => d.projectId === project.id);
    if (dirty.length) setPending({ project, dirty });
    else void unpin(project);
  };

  const onContextMenu = (e: MouseEvent, project: Project) => {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, project });
  };

  return (
    <aside className="sidebar">
      <div className="side-head">
        <span>Pinned projects</span>
        <span className="count">{projects.length}</span>
      </div>
      <ul className="projects">
        {projects.map((p) => {
          const agent = live.get(p.id);
          // A root that is gone (plan-16) cannot spawn a shell.
          const noSpawn = atCap || p.missing;
          const spawnTitle = p.missing ? MISSING_TITLE : atCap ? `Soft cap of ${PANE_CAP} reached` : "New terminal here";
          return (
            <li key={p.id} className="project-row">
              <button
                className={`project${p.id === activeId ? " active" : ""}`}
                onClick={() => setActive(p.id)}
                onContextMenu={(e) => onContextMenu(e, p)}
                title={p.missing ? MISSING_TITLE : p.path}
              >
                <Icon name="folder" />
                <span style={{ minWidth: 0 }}>
                  <div className="name">{p.name}</div>
                  <div className={`path${p.missing ? " missing" : ""}`}>{tilde(p.path)}</div>
                </span>
                <span className={`dot${agent ? " live" : ""}`} title={agent ? `${agent} running` : undefined} />
                <span
                  className="spawn"
                  role="button"
                  tabIndex={0}
                  title={spawnTitle}
                  aria-disabled={noSpawn}
                  onClick={(e) => {
                    e.stopPropagation();
                    if (!noSpawn) spawn(p);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      e.stopPropagation();
                      if (!noSpawn) spawn(p);
                    }
                  }}
                >
                  +
                </span>
              </button>
            </li>
          );
        })}
      </ul>
      <button className="add-project" onClick={onAddProject}>
        <span className="plus">+</span>Add project
      </button>
      <div className="side-foot">
        <button
          className={`settings-btn${view === "settings" ? " active" : ""}`}
          onClick={toggleSettings}
        >
          <Icon name="gear" />Settings
        </button>
      </div>

      {menu && (
        <div
          className="ctx-menu"
          role="menu"
          style={clampMenu(menu.x, menu.y, 1)}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <button role="menuitem" onClick={() => requestUnpin(menu.project)}>
            Unpin {menu.project.name}
          </button>
        </div>
      )}

      {pending && (
        <div className="scrim open" onPointerDown={(e) => e.target === e.currentTarget && setPending(null)}>
          <div className="modal" role="dialog" aria-modal="true">
            <h3>Unpin {pending.project.name}?</h3>
            <div className="field">
              <span>
                {pending.dirty.length === 1 ? "A file has" : `${pending.dirty.length} files have`} unsaved changes. Unpinning
                drops them; Ctrl+S first to keep them.
              </span>
              <ul className="dirty-list mono">
                {pending.dirty.map((d) => (
                  <li key={d.path}>{relativeTo(d.root, d.path)}</li>
                ))}
              </ul>
            </div>
            <div className="row">
              <button type="button" className="btn ghost" onClick={() => setPending(null)} autoFocus>
                Cancel
              </button>
              <button type="button" className="btn" onClick={() => unpin(pending.project)}>
                Discard and unpin
              </button>
            </div>
          </div>
        </div>
      )}
    </aside>
  );
}
