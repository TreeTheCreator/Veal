// The watcher's two events (src-tauri/src/watch.rs, plan-07): `fs-changed`
// batches fan out to the Files and Tasks stores, which refresh only what
// they show; `fs-watch-error` is a root that could not be watched — the pin
// stands, Refresh still works, so it is a toast, not a modal. A batch for a
// project with a live agent also lights the Files tree (files/activity.ts),
// and the agent's latest file starts fading once no agent runs there.
import { listen } from "@tauri-apps/api/event";
import { useActivity } from "./files/activity";
import { useFiles, type Batch } from "./files/store";
import { useLayout } from "./layout/store";
import { liveProjectIds, useProjects } from "./projects/store";
import { useTasks } from "./tasks/store";
import { useUi } from "./ui";

interface WatchError {
  projectId: number;
  error: string;
}

let started = false;

export function startWatching(): void {
  if (started) return;
  started = true;
  void listen<Batch>("fs-changed", ({ payload }) => {
    useFiles.getState().onChanged(payload);
    const project = useProjects.getState().projects.find((p) => p.id === payload.projectId);
    if (project) useTasks.getState().onChanged(project.id, project.path, payload.paths);
    if (project && payload.paths) {
      const { tree, agents } = useLayout.getState();
      if (liveProjectIds(tree, [project], agents).has(project.id)) {
        useActivity.getState().touch(project.id, project.path, payload.paths);
      }
    }
  });
  useLayout.subscribe(({ tree, agents }) => {
    const activity = useActivity.getState();
    if (activity.byProject.size === 0) return;
    const live = liveProjectIds(tree, useProjects.getState().projects, agents);
    for (const id of activity.byProject.keys()) if (!live.has(id)) activity.settle(id);
  });
  void listen<WatchError>("fs-watch-error", ({ payload }) => {
    const name = useProjects.getState().projects.find((p) => p.id === payload.projectId)?.name ?? `project ${payload.projectId}`;
    useUi.getState().showToast(`Not watching ${name}: ${payload.error} — use Refresh`);
  });
}
