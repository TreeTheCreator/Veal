// The mockup's "Add project" modal plus a native folder picker. Blank root →
// `~\name` is created; blank name → basename(root).
import { useEffect, useRef, useState, type FormEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { homeDir } from "@tauri-apps/api/path";
import { db, type PathKind } from "../db";
import { deriveProject, joinPath, normalizePath, tilde } from "../paths";
import { useProjects } from "../projects/store";
import { useUi } from "../ui";

interface Props {
  onClose(): void;
}

export function AddProjectModal({ onClose }: Props) {
  const add = useProjects((s) => s.add);
  const showToast = useUi((s) => s.showToast);
  const [name, setName] = useState("");
  const [root, setRoot] = useState("");
  const [home, setHome] = useState("");
  const [kind, setKind] = useState<PathKind | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    nameRef.current?.focus();
    homeDir().then((h) => setHome(normalizePath(h))).catch((e) => console.warn("homeDir", e));
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  // Existence check for the hint under the root field.
  useEffect(() => {
    const r = normalizePath(root);
    if (!r) {
      setKind(null);
      return;
    }
    let stale = false;
    db.pathKind(r).then(
      (k) => {
        if (!stale) setKind(k);
      },
      () => {
        if (!stale) setKind(null);
      },
    );
    return () => {
      stale = true;
    };
  }, [root]);

  const draft = deriveProject(name, root, home);
  const placeholder = joinPath(home || "~", name.trim() || "my-project");
  const hint = !normalizePath(root)
    ? `Leave blank to create it in ${tilde(home) || "~"}`
    : kind === "file"
      ? "That path is a file, not a directory"
      : kind === "missing"
        ? "Will be created"
        : "";

  const browse = async () => {
    const picked = await open({ directory: true, multiple: false, title: "Project root" });
    if (typeof picked === "string") {
      setRoot(picked);
      setError(null);
    }
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!draft) {
      nameRef.current?.focus();
      return;
    }
    if (kind === "file") {
      setError("That path is a file, not a directory");
      return;
    }
    setBusy(true);
    try {
      const project = await add(draft.name, draft.path);
      showToast(draft.created ? `Created ${tilde(project.path)} and pinned it` : `Pinned ${tilde(project.path)}`);
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="scrim open" onPointerDown={(e) => e.target === e.currentTarget && onClose()}>
      <form className="modal" onSubmit={submit}>
        <h3>Add project</h3>
        <label className="field">
          Name
          <input
            ref={nameRef}
            type="text"
            placeholder="my-project"
            autoComplete="off"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        <label className="field">
          <span>
            Root directory <span style={{ color: "var(--muted)" }}>(optional)</span>
          </span>
          <span className="field-row">
            <input
              type="text"
              className="mono"
              autoComplete="off"
              placeholder={placeholder}
              value={root}
              onChange={(e) => {
                setRoot(e.target.value);
                setError(null);
              }}
            />
            <button type="button" className="btn" onClick={browse}>
              Browse…
            </button>
          </span>
          {error ? <small className="error">{error}</small> : <small>{hint}</small>}
        </label>
        <div className="row">
          <button type="button" className="btn ghost" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="btn primary" disabled={busy}>
            Pin project
          </button>
        </div>
      </form>
    </div>
  );
}
