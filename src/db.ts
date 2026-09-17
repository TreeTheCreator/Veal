// Thin wrapper over the persistence commands (src-tauri/src/db.rs) so stores
// never call `invoke` directly, mirroring pty.ts.
import { invoke } from "@tauri-apps/api/core";

export interface Project {
  id: number;
  name: string;
  path: string;
}

export type PathKind = "dir" | "file" | "missing";

export const db = {
  projectsList: () => invoke<Project[]>("projects_list"),
  projectsAdd: (name: string, path: string) => invoke<Project>("projects_add", { name, path }),
  projectsRemove: (id: number) => invoke<void>("projects_remove", { id }),
  settingsGetAll: () => invoke<Record<string, string>>("settings_get_all"),
  settingsSet: (key: string, value: string) => invoke<void>("settings_set", { key, value }),
  pathKind: (path: string) => invoke<PathKind>("path_kind", { path }),
};
