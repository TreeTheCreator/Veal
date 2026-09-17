//! Agent auto-detection (plan-08): which panes are actually running an
//! agent CLI. A poller thread refreshes the process table every `TICK`,
//! walks the descendants of each pane's shell, and reports a change per
//! pane as `PaneAgent`. The manual Shell / Agent label is untouched — this
//! is a runtime overlay, never persisted.
//!
//! Tauri-agnostic like `watch.rs`: `detect` and `tick` are pure over a
//! `Proc` table and a sink, so `cargo test` covers the rules and the
//! change-only behaviour without a webview.

use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::pty::PtyRegistry;

/// Exe stems (lowercase, no extension) that count as an agent.
pub const AGENTS: [&str; 8] = ["claude", "codex", "aider", "gemini", "copilot", "amp", "goose", "opencode"];
/// Interpreters whose *script* decides: `node …\gemini.js` is gemini.
const INTERPRETERS: [&str; 6] = ["node", "bun", "deno", "python", "python3", "pythonw"];
pub const TICK: Duration = Duration::from_millis(1500);

/// One process, as the poller sees it.
#[derive(Debug, Clone)]
pub struct Proc {
    pub pid: u32,
    pub ppid: Option<u32>,
    /// Exe stem, lowercase: `claude.exe` → `claude`.
    pub name: String,
    pub cmd: Vec<String>,
    /// Seconds since the epoch; guards against parent-pid reuse.
    pub start: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaneAgent {
    pub id: u32,
    pub agent: Option<String>,
}

/// `C:\x\claude.exe` or `claude` → `claude`; lowercase.
pub fn stem(s: &str) -> String {
    Path::new(s)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn agent_named(stem: &str) -> Option<&'static str> {
    AGENTS.iter().copied().find(|a| *a == stem)
}

/// The agent a single process is, if any: by exe stem, or for an
/// interpreter by the stem of any argument (`…\gemini.js`).
pub fn agent_of(p: &Proc) -> Option<&'static str> {
    if let Some(a) = agent_named(&p.name) {
        return Some(a);
    }
    if INTERPRETERS.contains(&p.name.as_str()) {
        return p.cmd.iter().skip(1).find_map(|arg| agent_named(&stem(arg)));
    }
    None
}

/// Children by parent pid. A link is dropped when the parent started after
/// the child — a reused pid, not a real parent.
pub fn children_index(table: &[Proc]) -> HashMap<u32, Vec<&Proc>> {
    let by_pid: HashMap<u32, &Proc> = table.iter().map(|p| (p.pid, p)).collect();
    let mut idx: HashMap<u32, Vec<&Proc>> = HashMap::new();
    for p in table {
        let Some(ppid) = p.ppid else { continue };
        let Some(parent) = by_pid.get(&ppid) else { continue };
        if parent.start > p.start {
            continue;
        }
        idx.entry(ppid).or_default().push(p);
    }
    idx
}

/// Descendants of `root`, breadth-first — nearest first.
pub fn descendants<'a>(idx: &HashMap<u32, Vec<&'a Proc>>, root: u32) -> Vec<&'a Proc> {
    let mut out = Vec::new();
    let mut queue = vec![root];
    let mut i = 0;
    while i < queue.len() {
        let pid = queue[i];
        i += 1;
        for c in idx.get(&pid).into_iter().flatten() {
            if !queue.contains(&c.pid) {
                queue.push(c.pid);
                out.push(*c);
            }
        }
    }
    out
}

/// Which agent, if any, runs under `root` (the pane's shell). Nearest wins.
/// `tick` inlines this to share one index across panes.
#[cfg(test)]
pub fn detect(table: &[Proc], root: u32) -> Option<&'static str> {
    let idx = children_index(table);
    descendants(&idx, root).into_iter().find_map(agent_of)
}

/// Runs `detect` for every pane and calls `sink` only for panes whose
/// answer changed since the last call. `last` is the poller's memory.
pub fn tick(
    table: &[Proc],
    panes: &[(u32, u32)],
    last: &mut HashMap<u32, Option<&'static str>>,
    sink: &impl Fn(PaneAgent),
) {
    let idx = children_index(table);
    let mut seen = HashMap::with_capacity(panes.len());
    for &(id, pid) in panes {
        let now = descendants(&idx, pid).into_iter().find_map(agent_of);
        let before = last.get(&id).copied().unwrap_or(None);
        if now != before {
            sink(PaneAgent { id, agent: now.map(str::to_string) });
        }
        seen.insert(id, now);
    }
    // A pane that went away starts over as `None` if its id ever comes back.
    *last = seen;
}

fn snapshot(sys: &mut System) -> Vec<Proc> {
    // Command lines are read once per process, not per tick.
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::OnlyIfNotSet),
    );
    sys.processes()
        .values()
        .map(|p| Proc {
            pid: p.pid().as_u32(),
            ppid: p.parent().map(|pp| pp.as_u32()),
            name: stem(&p.name().to_string_lossy()),
            cmd: p.cmd().iter().map(|c| c.to_string_lossy().into_owned()).collect(),
            start: p.start_time(),
        })
        .collect()
}

/// Starts the poller thread. Idle (no panes) ticks skip the refresh.
pub fn start(registry: Arc<PtyRegistry>, sink: impl Fn(PaneAgent) + Send + 'static) {
    std::thread::Builder::new()
        .name("veal-agents".into())
        .spawn(move || {
            let mut sys = System::new();
            let mut last = HashMap::new();
            loop {
                let panes = registry.pids();
                if panes.is_empty() {
                    if !last.is_empty() {
                        last.clear();
                    }
                } else {
                    let t = Instant::now();
                    let table = snapshot(&mut sys);
                    if cfg!(debug_assertions) {
                        let ms = t.elapsed().as_secs_f64() * 1000.0;
                        if ms > 50.0 {
                            eprintln!("[agents] refresh {} procs in {ms:.1} ms", table.len());
                        }
                    }
                    tick(&table, &panes, &mut last, &sink);
                }
                std::thread::sleep(TICK);
            }
        })
        .expect("spawn agents thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, ppid: Option<u32>, name: &str, cmd: &[&str]) -> Proc {
        Proc { pid, ppid, name: stem(name), cmd: cmd.iter().map(|s| s.to_string()).collect(), start: 100 }
    }

    #[test]
    fn stems() {
        assert_eq!(stem(r"C:\x\claude.exe"), "claude");
        assert_eq!(stem("Claude"), "claude");
        assert_eq!(stem(r"C:\npm\node_modules\@google\gemini-cli\dist\gemini.js"), "gemini");
    }

    #[test]
    fn detects_by_exe_stem_and_by_interpreter_script() {
        let t = vec![
            p(1, None, "pwsh.exe", &[]),
            p(2, Some(1), "claude.exe", &[r"C:\x\claude.exe"]),
            p(10, None, "pwsh.exe", &[]),
            p(11, Some(10), "cmd.exe", &[]),
            p(12, Some(11), "node.exe", &["node", r"C:\npm\gemini.js"]),
            p(20, None, "pwsh.exe", &[]),
            p(21, Some(20), "node.exe", &["node", r"C:\veal\node_modules\vite\bin\vite.js"]),
            p(30, None, "pwsh.exe", &[]),
        ];
        assert_eq!(detect(&t, 1), Some("claude"));
        assert_eq!(detect(&t, 10), Some("gemini"));
        assert_eq!(detect(&t, 20), None);
        assert_eq!(detect(&t, 30), None);
        // Another root's agent is not ours.
        assert_eq!(detect(&t, 20), None);
    }

    #[test]
    fn nearest_agent_wins_and_stems_must_match_exactly() {
        let t = vec![
            p(1, None, "pwsh.exe", &[]),
            p(2, Some(1), "codex.exe", &[]),
            p(3, Some(2), "claude.exe", &[]),
            p(4, Some(1), "nodemon.exe", &["nodemon", r"C:\x\aider.js"]),
            p(5, Some(1), "claude-helper.exe", &[]),
        ];
        assert_eq!(detect(&t, 1), Some("codex"));
        assert_eq!(agent_of(&t[3]), None, "nodemon is not an interpreter");
        assert_eq!(agent_of(&t[4]), None, "claude-helper is not claude");
    }

    #[test]
    fn a_parent_newer_than_its_child_is_a_reused_pid() {
        let mut t = vec![p(1, None, "pwsh.exe", &[]), p(2, Some(1), "claude.exe", &[])];
        t[0].start = 200; // the shell "started" after claude: pid 1 was reused
        assert_eq!(detect(&t, 1), None);
    }

    #[test]
    fn tick_reports_changes_only() {
        let shell = p(1, None, "pwsh.exe", &[]);
        let with_agent = vec![shell.clone(), p(2, Some(1), "claude.exe", &[])];
        let without = vec![shell.clone()];
        let panes = [(7u32, 1u32), (8, 99)];
        let mut last = HashMap::new();
        let got = std::cell::RefCell::new(Vec::new());
        let sink = |a: PaneAgent| got.borrow_mut().push(a);
        tick(&without, &panes, &mut last, &sink);
        tick(&with_agent, &panes, &mut last, &sink);
        tick(&with_agent, &panes, &mut last, &sink);
        tick(&without, &panes, &mut last, &sink);
        assert_eq!(
            got.into_inner(),
            vec![
                PaneAgent { id: 7, agent: Some("claude".into()) },
                PaneAgent { id: 7, agent: None },
            ]
        );
        // Pane 8 never had an agent and was never reported.
        assert_eq!(last.get(&8), Some(&None));
    }

    /// Real pids: spawn a shell, start `ping`, and find it under the shell.
    #[test]
    fn real_process_tree_has_the_shells_child() {
        use crate::pty::{PtyRegistry, SpawnOptions};
        use std::sync::mpsc;
        use std::time::Instant;

        let registry = Arc::new(PtyRegistry::default());
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let shell = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
        let id = registry
            .spawn(
                SpawnOptions {
                    cwd: std::env::temp_dir().to_string_lossy().into_owned(),
                    cols: 80,
                    rows: 24,
                    shell: Some(shell.into()),
                },
                move |c| {
                    let _ = tx.send(c);
                },
                |_, _| {},
            )
            .expect("spawn");
        let (_, pid) = registry.pids().into_iter().find(|(i, _)| *i == id).expect("pid recorded");

        // Play the terminal for ConPTY (see pty.rs), then wait for a prompt.
        let mut out = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut answered = false;
        while Instant::now() < deadline {
            if let Ok(c) = rx.recv_timeout(Duration::from_millis(200)) {
                out.extend(c);
            }
            if !answered && out.windows(4).any(|w| w == b"\x1b[6n") {
                registry.write(id, b"\x1b[1;1R").unwrap();
                answered = true;
            }
            if out.contains(&b'>') || out.contains(&b'$') {
                break;
            }
        }
        let cmd: &[u8] = if cfg!(windows) { b"ping -n 30 127.0.0.1\r\n" } else { b"sleep 30\n" };
        registry.write(id, cmd).unwrap();

        let want = if cfg!(windows) { "ping" } else { "sleep" };
        let mut sys = System::new();
        let mut found = false;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && !found {
            std::thread::sleep(Duration::from_millis(300));
            let table = snapshot(&mut sys);
            let idx = children_index(&table);
            found = descendants(&idx, pid).iter().any(|p| p.name == want);
        }
        // Ending the shell also ends ping; the kill's own result is not under test.
        let _ = registry.kill(id);
        assert!(found, "{want} not found under shell pid {pid}");
    }
}
