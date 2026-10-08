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
/// Package directories that are an agent under another name (plan-12):
/// `…\node_modules\@anthropic-ai\claude-code\cli.js` is claude.
const PACKAGES: [(&str, &str); 2] = [("claude-code", "claude"), ("gemini-cli", "gemini")];
/// Only components after one of these count as a package name; anything
/// before is the user's own tree (`C:\dev\claude\build.js` is not claude).
const PACKAGE_ROOTS: [&str; 2] = ["node_modules", "site-packages"];
/// 3 s (plan-16): a release-build refresh of the process table measured
/// p50 16.7 ms / max 25.9 ms idle, so 1.5 s was 1.1 % of a core for nothing.
pub const TICK: Duration = Duration::from_millis(3000);

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

/// The agent a package directory names, if any: an agent's own name or
/// one of `PACKAGES`.
fn agent_packaged(component: &str) -> Option<&'static str> {
    agent_named(component).or_else(|| PACKAGES.iter().find(|(pkg, _)| *pkg == component).map(|(_, a)| *a))
}

/// The agent an interpreter's script argument names: its stem, or a
/// package directory on its path after the last `node_modules` /
/// `site-packages` (the npm shim for claude runs `…\claude-code\cli.js`).
pub fn agent_of_script(arg: &str) -> Option<&'static str> {
    if let Some(a) = agent_named(&stem(arg)) {
        return Some(a);
    }
    let components: Vec<String> = Path::new(arg)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let start = components.iter().rposition(|c| PACKAGE_ROOTS.contains(&c.as_str()))?;
    components[start + 1..].iter().find_map(|c| agent_packaged(c))
}

/// The agent a single process is, if any: by exe stem, or for an
/// interpreter by any argument (`agent_of_script`).
pub fn agent_of(p: &Proc) -> Option<&'static str> {
    if let Some(a) = agent_named(&p.name) {
        return Some(a);
    }
    if INTERPRETERS.contains(&p.name.as_str()) {
        return p.cmd.iter().skip(1).find_map(|arg| agent_of_script(arg));
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
                        if ms > 10.0 {
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
    fn interpreter_scripts_match_by_package_directory() {
        // Plan-12: the npm shim for claude is `cli.js` under the package dir.
        assert_eq!(agent_of_script(r"C:\Users\u\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\cli.js"), Some("claude"));
        assert_eq!(agent_of_script(r"C:\npm\node_modules\@openai\codex\bin\codex.js"), Some("codex"));
        assert_eq!(agent_of_script(r"C:\npm\node_modules\@google\gemini-cli\dist\index.js"), Some("gemini"));
        assert_eq!(agent_of_script(r"C:\py\Lib\site-packages\aider\__main__.py"), Some("aider"));
        assert_eq!(agent_of_script("/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js"), Some("claude"));
        // The user's own tree never matches; only the part after node_modules does.
        assert_eq!(agent_of_script(r"C:\dev\claude\build.js"), None);
        assert_eq!(agent_of_script(r"C:\dev\claude\node_modules\vite\bin\vite.js"), None);
        assert_eq!(agent_of_script(r"C:\dev\claude-experiments\node_modules\esbuild\bin\esbuild"), None);
        let node = p(2, Some(1), "node.exe", &["node", r"C:\npm\node_modules\@anthropic-ai\claude-code\cli.js"]);
        assert_eq!(agent_of(&node), Some("claude"));
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

    /// The poller's cost (plan-16, section 7): 40 refreshes at `TICK`, one
    /// idle minute. Run by hand with `cargo test --release -- --ignored
    /// --nocapture poller_cost`; the p50 decides whether `TICK` changes.
    #[test]
    #[ignore]
    fn poller_cost() {
        let mut sys = System::new();
        let mut ms = Vec::new();
        for _ in 0..40 {
            let t = Instant::now();
            let table = snapshot(&mut sys);
            ms.push(t.elapsed().as_secs_f64() * 1000.0);
            std::thread::sleep(TICK);
            let _ = table.len();
        }
        ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        eprintln!(
            "[poller_cost] p50 {:.2} ms, max {:.2} ms, per tick {:.2}% of a core",
            ms[ms.len() / 2],
            ms[ms.len() - 1],
            ms[ms.len() / 2] / TICK.as_secs_f64() / 10.0
        );
    }

    /// Real pids: spawn a shell, start `ping`, and find it under the shell.
    #[test]
    fn real_process_tree_has_the_shells_child() {
        use crate::pty::{PtyRegistry, SpawnOptions};
        use std::sync::mpsc;
        use std::time::Instant;

        let registry = Arc::new(PtyRegistry::default());
        let (tx, rx) = mpsc::channel::<u32>();
        let shell = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
        let id = registry
            .spawn(
                SpawnOptions {
                    cwd: std::env::temp_dir().to_string_lossy().into_owned(),
                    cols: 80,
                    rows: 24,
                    shell: Some(shell.into()),
                    proxy: None,
                },
                move |id| {
                    let _ = tx.send(id);
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
            let _ = rx.recv_timeout(Duration::from_millis(50));
            out.extend(registry.take(id));
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
