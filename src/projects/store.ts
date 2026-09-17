// Pinned projects and which one is active. Every mutation writes through to
// SQLite first, so the store only ever reflects what is on disk.
import { create } from "zustand";
import { db, type Project } from "../db";
import { useFiles } from "../files/store";
import { panes, type LayoutNode } from "../layout/tree";
import { samePath } from "../paths";
import { useTasks } from "../tasks/store";

interface ProjectsState {
  projects: Project[];
  activeId: number | null;
  loaded: boolean;
  load(): Promise<void>;
  add(name: string, path: string): Promise<Project>;
  remove(id: number): Promise<void>;
  setActive(id: number): void;
}

export const useProjects = create<ProjectsState>((set, get) => ({
  projects: [],
  activeId: null,
  loaded: false,

  async load() {
    const [projects, settings] = await Promise.all([db.projectsList(), db.settingsGetAll()]);
    const saved = Number(settings.active_project);
    const activeId = projects.some((p) => p.id === saved) ? saved : (projects[0]?.id ?? null);
    set({ projects, activeId, loaded: true });
  },

  async add(name, path) {
    const project = await db.projectsAdd(name, path);
    set({ projects: [...get().projects, project] });
    get().setActive(project.id);
    return project;
  },

  async remove(id) {
    // Rust clears `active_project` if it pointed here; pick the next one ourselves.
    await db.projectsRemove(id);
    useFiles.getState().forget(id);
    useTasks.getState().forget(id);
    const projects = get().projects.filter((p) => p.id !== id);
    if (get().activeId !== id) {
      set({ projects });
      return;
    }
    const next = projects[0]?.id ?? null;
    set({ projects, activeId: next });
    if (next !== null) get().setActive(next);
  },

  setActive(id) {
    set({ activeId: id });
    db.settingsSet("active_project", String(id)).catch((e) => console.warn("settings_set", e));
  },
}));

export const selectActive = (s: ProjectsState): Project | null =>
  s.projects.find((p) => p.id === s.activeId) ?? null;

/** Project whose root is `cwd`, for the pane header; null when a pane is elsewhere. */
export function projectForCwd(projects: Project[], cwd: string): Project | null {
  return projects.find((p) => samePath(p.path, cwd)) ?? null;
}

/**
 * Ids of projects with a live agent pane: an `agent` pane whose spawn cwd is
 * the project root. Computed from cwd only — the multiplexer does not know
 * about projects (spec), so there is no projectId on panes.
 */
export function liveProjectIds(tree: LayoutNode | null, projects: Project[]): Set<number> {
  const agents = panes(tree).filter((p) => p.kind === "agent");
  const live = new Set<number>();
  for (const project of projects) {
    if (agents.some((p) => samePath(p.cwd, project.path))) live.add(project.id);
  }
  return live;
}
