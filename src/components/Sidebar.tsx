// Pinned projects (mockup `renderSidebar`): click selects, hover `+` opens the
// type popover for that project, the dot shows a live agent pane, right-click
// unpins. The Settings entry is pinned to the foot.
import { useEffect, useMemo, useState, type MouseEvent } from "react";
import { Icon } from "../Icon";
import type { Project } from "../db";
import { useLayout } from "../layout/store";
import { PANE_CAP, panes, type PaneKind } from "../layout/tree";
import { tilde } from "../paths";
import { clampMenu } from "../menu";
import { liveProjectIds, useProjects } from "../projects/store";
import { useSettings } from "../settings/store";
import { useUi } from "../ui";
import { TypePopover } from "./TypePopover";

interface Props {
  onAddProject(): void;
}

interface Menu {
  x: number;
  y: number;
  project: Project;
}

export function Sidebar({ onAddProject }: Props) {
  const projects = useProjects((s) => s.projects);
  const activeId = useProjects((s) => s.activeId);
  const setActive = useProjects((s) => s.setActive);
  const remove = useProjects((s) => s.remove);
  const tree = useLayout((s) => s.tree);
  const agents = useLayout((s) => s.agents);
  const addPane = useLayout((s) => s.addPane);
  const defaultKind = useSettings((s) => s.defaultKind);
  const view = useUi((s) => s.view);
  const setView = useUi((s) => s.setView);
  const showToast = useUi((s) => s.showToast);

  const live = useMemo(() => liveProjectIds(tree, projects, agents), [tree, projects, agents]);
  const atCap = panes(tree).length >= PANE_CAP;

  const [spawnFor, setSpawnFor] = useState<number | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);

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

  const spawn = (project: Project, kind: PaneKind) => {
    setSpawnFor(null);
    if (view !== "terminal") setView("terminal");
    if (addPane(kind, project.path) === null) showToast(`Soft cap of ${PANE_CAP} reached`);
  };

  const unpin = async (project: Project) => {
    setMenu(null);
    try {
      await remove(project.id);
      showToast(`Unpinned ${project.name}`);
    } catch (e) {
      showToast(`Could not unpin: ${String(e)}`);
    }
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
          return (
            <li key={p.id} className="project-row">
              <button
                className={`project${p.id === activeId ? " active" : ""}`}
                onClick={() => setActive(p.id)}
                onContextMenu={(e) => onContextMenu(e, p)}
                title={p.path}
              >
                <Icon name="folder" />
                <span style={{ minWidth: 0 }}>
                  <div className="name">{p.name}</div>
                  <div className="path">{tilde(p.path)}</div>
                </span>
                <span className={`dot${agent ? " live" : ""}`} title={agent ? `${agent} running` : undefined} />
                <span
                  className="spawn"
                  role="button"
                  tabIndex={0}
                  title={atCap ? `Soft cap of ${PANE_CAP} reached` : "New terminal here"}
                  aria-disabled={atCap}
                  onClick={(e) => {
                    e.stopPropagation();
                    if (!atCap) setSpawnFor(p.id);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      e.stopPropagation();
                      if (!atCap) setSpawnFor(p.id);
                    }
                  }}
                >
                  +
                </span>
              </button>
              {spawnFor === p.id && (
                <TypePopover
                  className="side"
                  cwdLabel={tilde(p.path)}
                  defaultKind={defaultKind}
                  onClose={() => setSpawnFor(null)}
                  onPick={(kind) => spawn(p, kind)}
                />
              )}
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
          onClick={() => setView("settings")}
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
          <button role="menuitem" onClick={() => unpin(menu.project)}>
            Unpin {menu.project.name}
          </button>
        </div>
      )}
    </aside>
  );
}
