// Tasks view (plan-05): the mockup's three-column board over the active
// project's `.veal/tasks.json` — or, in global scope, every pinned project's
// file plus the Inbox. Re-reads its scope whenever it is shown, since agents
// write these files and there is no watcher.
import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import type { Project } from "../db";
import { tilde } from "../paths";
import { selectActive, useProjects } from "../projects/store";
import { STATUSES, type Task } from "../tasks/api";
import { backward, COLUMN_LABEL, columns, forward, isEmpty, today, type Card, type Source } from "../tasks/board";
import { useTasks, type ListState } from "../tasks/store";
import { useUi } from "../ui";
import { InlineInput } from "./InlineInput";

const INBOX = "__inbox__";

export function TasksView({ visible }: { visible: boolean }) {
  const projects = useProjects((s) => s.projects);
  const active = useProjects(selectActive);
  const loaded = useProjects((s) => s.loaded);
  const showToast = useUi((s) => s.showToast);
  const { byProject, inbox, load, add, setStatus, setTitle, shift, remove, setScope } = useTasks();
  // Plan-10: the one card whose title is being edited, keyed `project:id`.
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const scopeSetting = useTasks((s) => s.scope);
  // Nothing pinned → only the Inbox exists, so the scope is forced global.
  const scope = active ? scopeSetting : "global";

  // Sources in display order: sidebar order, Inbox last.
  const sources = useMemo<Source[]>(() => {
    const list = (id: number | null): ListState =>
      id === null ? inbox : (byProject.get(id) ?? { status: "loaded", tasks: [] });
    const visibleProjects = scope === "project" && active ? [active] : projects;
    return [
      ...visibleProjects.map((p) => ({ project: p as Project | null, tasks: list(p.id).tasks })),
      ...(scope === "global" ? [{ project: null, tasks: inbox.tasks }] : []),
    ];
  }, [scope, active, projects, byProject, inbox]);

  const errors = useMemo(() => {
    const out: Array<{ name: string; error: string }> = [];
    for (const { project } of sources) {
      const s = project === null ? inbox : byProject.get(project.id);
      if (s?.status === "error") out.push({ name: project?.name ?? "Inbox", error: s.error });
    }
    return out;
  }, [sources, byProject, inbox]);

  const reload = () => {
    for (const { project } of sources) void load(project?.id ?? null, project?.path ?? null);
  };

  // Re-read the scope on show, and on active-project / scope change while shown.
  const reloadRef = useRef(reload);
  reloadRef.current = reload;
  useEffect(() => {
    if (visible && loaded) reloadRef.current();
  }, [visible, loaded, scope, active?.id, projects.length]);

  const cols = useMemo(() => columns(sources, scope), [sources, scope]);
  const rootFor = (p: Project | null) => (p ? p.path : null);
  const onForward = (c: Card) =>
    void setStatus(c.project?.id ?? null, rootFor(c.project), c.task.id, forward(c.task.status));
  const onReopen = (c: Card) => void setStatus(c.project?.id ?? null, rootFor(c.project), c.task.id, "todo");
  const onBack = (c: Card) => void setStatus(c.project?.id ?? null, rootFor(c.project), c.task.id, backward(c.task.status));
  const onShift = (c: Card, delta: -1 | 1) => void shift(c.project?.id ?? null, rootFor(c.project), c.task.id, delta);
  const onRemove = (c: Card) => void remove(c.project?.id ?? null, rootFor(c.project), c.task.id);
  const cardKey = (c: Card) => `${c.project?.id ?? "inbox"}:${c.task.id}`;
  const onRename = async (c: Card, title: string): Promise<boolean> => {
    if (title.trim() === c.task.title) {
      setEditingKey(null);
      return true;
    }
    const ok = await setTitle(c.project?.id ?? null, rootFor(c.project), c.task.id, title);
    if (ok) setEditingKey(null);
    else showToast("Could not rename the task");
    return ok;
  };

  const loading = sources.some(({ project }) => (project === null ? inbox : byProject.get(project.id))?.status === "loading");
  const activeError = active ? byProject.get(active.id)?.status === "error" : false;
  const formDisabled = scope === "project" && activeError;

  const storage =
    scope === "project" && active ? (
      <span className="storage">
        {tilde(active.path)}/.veal/tasks.json <span className="git">· git-tracked</span>
      </span>
    ) : (
      <span className="storage">
        {projects.length} project file{projects.length === 1 ? "" : "s"} + Inbox
      </span>
    );

  return (
    <>
      <div className="toolbar">
        <div className="seg" role="radiogroup">
          <button
            className={scope === "project" ? "on" : ""}
            disabled={!active}
            onClick={() => setScope("project")}
          >
            {active?.name ?? "This project"}
          </button>
          <button className={scope === "global" ? "on" : ""} onClick={() => setScope("global")}>
            All projects
          </button>
        </div>
        {storage}
        <button className="btn" onClick={reload} title="Re-read the task files in this scope">
          Refresh
        </button>
        <span className="grow" />
        <span className="hint">◂ ▸ move a task · ↑ ↓ reorder · double-click a title to edit it</span>
      </div>

      {errors.length > 0 && (
        <div className="tasks-error">
          {errors.map((e) => (
            <div key={e.name}>
              {e.name}: {e.error}
            </div>
          ))}
        </div>
      )}

      {isEmpty(cols) ? (
        <div className="empty">{loaded && !loading ? "No tasks yet — add one below." : ""}</div>
      ) : (
        <div className="board">
          {STATUSES.map((status) => (
            <div className="tasks-col" key={status}>
              <div className="col-head">
                <span>{COLUMN_LABEL[status]}</span>
                <span className="n">{cols[status].length}</span>
              </div>
              {cols[status].map((c, i, all) => (
                <TaskCard
                  key={cardKey(c)}
                  card={c}
                  editing={editingKey === cardKey(c)}
                  // ↑/↓ swap within one source; a card is at an edge when no card of the same source is beside it.
                  canUp={all.slice(0, i).some((o) => o.project?.id === c.project?.id)}
                  canDown={all.slice(i + 1).some((o) => o.project?.id === c.project?.id)}
                  onEdit={() => setEditingKey(cardKey(c))}
                  onCancelEdit={() => setEditingKey(null)}
                  onRename={(title) => onRename(c, title)}
                  onShift={(d) => onShift(c, d)}
                  onBack={onBack}
                  onForward={onForward}
                  onReopen={onReopen}
                  onRemove={onRemove}
                />
              ))}
            </div>
          ))}
        </div>
      )}

      <AddTaskForm
        projects={projects}
        active={active}
        disabled={formDisabled}
        onAdd={async (project, title) => {
          const ok = await add(project?.id ?? null, rootFor(project), title, today());
          if (!ok) return false;
          showToast(project ? `Written to ${tilde(project.path)}/.veal/tasks.json` : "Saved to Inbox (veal.db)");
          // Adding to another project while in project scope: flip to global so the card is visible.
          if (scope === "project" && project?.id !== active?.id) setScope("global");
          return true;
        }}
      />
    </>
  );
}

const emptyTitle = (v: string) => (v.trim() === "" ? "a task needs a title" : null);

function TaskCard({
  card,
  editing,
  canUp,
  canDown,
  onEdit,
  onCancelEdit,
  onRename,
  onShift,
  onBack,
  onForward,
  onReopen,
  onRemove,
}: {
  card: Card;
  editing: boolean;
  canUp: boolean;
  canDown: boolean;
  onEdit: () => void;
  onCancelEdit: () => void;
  onRename: (title: string) => Promise<boolean>;
  onShift: (delta: -1 | 1) => void;
  onBack: (c: Card) => void;
  onForward: (c: Card) => void;
  onReopen: (c: Card) => void;
  onRemove: (c: Card) => void;
}) {
  const t: Task = card.task;
  return (
    <div className={`task${t.status === "done" ? " done" : ""}${editing ? " editing" : ""}`}>
      {editing ? (
        <InlineInput className="title-edit" initial={t.title} validate={emptyTitle} onCommit={onRename} onCancel={onCancelEdit} />
      ) : (
        <div className="title" onDoubleClick={onEdit} title="Double-click to edit">{t.title}</div>
      )}
      <div className="meta">
        {card.chip && <span className={`chip${card.project === null ? " inbox" : ""}`}>{card.chip}</span>}
        {!editing && (
          <span className="acts">
            <button title="Edit title" onClick={onEdit}>✎</button>
            <button title="Move up" disabled={!canUp} onClick={() => onShift(-1)}>↑</button>
            <button title="Move down" disabled={!canDown} onClick={() => onShift(1)}>↓</button>
            {t.status !== "todo" && (
              <button title="Move back" onClick={() => onBack(card)}>◂</button>
            )}
            {t.status !== "done" ? (
              <button title="Move forward" onClick={() => onForward(card)}>▸</button>
            ) : (
              <button title="Reopen" onClick={() => onReopen(card)}>↺</button>
            )}
            <button title="Delete" onClick={() => onRemove(card)}>×</button>
          </span>
        )}
      </div>
    </div>
  );
}

function AddTaskForm({
  projects,
  active,
  disabled,
  onAdd,
}: {
  projects: Project[];
  active: Project | null;
  disabled: boolean;
  onAdd: (project: Project | null, title: string) => Promise<boolean>;
}) {
  const [title, setTitle] = useState("");
  const [target, setTarget] = useState<string>(active ? String(active.id) : INBOX);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Follow the active project until the user picks something else.
  const lastActive = useRef(active?.id);
  useEffect(() => {
    if (active?.id !== lastActive.current) {
      lastActive.current = active?.id;
      setTarget(active ? String(active.id) : INBOX);
    }
  }, [active]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = title.trim();
    if (!trimmed || busy || disabled) return;
    const project = target === INBOX ? null : (projects.find((p) => String(p.id) === target) ?? null);
    setBusy(true);
    const ok = await onAdd(project, trimmed);
    setBusy(false);
    if (ok) setTitle("");
    inputRef.current?.focus();
  };

  return (
    <form className="add-task" onSubmit={submit}>
      <input
        ref={inputRef}
        type="text"
        placeholder={disabled ? "Fix tasks.json, then Refresh" : "New task…"}
        autoComplete="off"
        value={title}
        disabled={disabled}
        onChange={(e) => setTitle(e.target.value)}
      />
      <select value={target} disabled={disabled} onChange={(e) => setTarget(e.target.value)}>
        {projects.map((p) => (
          <option key={p.id} value={String(p.id)}>{p.name}</option>
        ))}
        <option value={INBOX}>Inbox</option>
      </select>
      <button className="btn" type="submit" disabled={disabled || busy || !title.trim()}>
        Add
      </button>
    </form>
  );
}
