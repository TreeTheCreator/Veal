//! File watching for every pinned project root (plan-07). One `notify`
//! watcher per root, recursive; a single coalescing thread turns the raw
//! event stream into one `Batch` per project per burst, so the frontend
//! sees "these paths changed" at most a few times a second and refreshes
//! only what it is showing.
//!
//! Tauri-agnostic like `fs.rs`: `coalesce` is pure and `Watchers` takes a
//! sink closure, so `cargo test` covers both with a tempdir.

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A burst ends once the channel has been silent this long...
pub const QUIET: Duration = Duration::from_millis(150);
/// ...or this long after its first event, whichever comes first.
pub const MAX_LATENCY: Duration = Duration::from_millis(1000);
/// More distinct paths than this in one batch → `paths: None`.
pub const CAP: usize = 256;

/// Temp files of our own atomic writes (`fs.rs`, `tasks.rs`); the rename to
/// the real name is the event that matters.
const TMP_SUFFIXES: [&str; 2] = [".veal-tmp", "tasks.json.tmp"];

/// One notify event, tagged with the project it came from.
#[derive(Debug, Clone)]
pub struct Raw {
    pub project: u32,
    pub root: PathBuf,
    pub paths: Vec<PathBuf>,
    /// notify's `Rescan` flag (kernel buffer overflow) or a watcher error.
    pub rescan: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub project_id: u32,
    /// Changed paths, absolute, deduplicated; `None` = too many or a rescan
    /// — refresh everything shown for this project.
    pub paths: Option<Vec<String>>,
}

fn is_noise(root: &Path, path: &Path) -> bool {
    if let Ok(rel) = path.strip_prefix(root) {
        if rel.components().next().map(|c| c.as_os_str() == ".git").unwrap_or(false) {
            return true;
        }
    }
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    TMP_SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// Groups a burst by project, drops `.git/` traffic and our temp files,
/// deduplicates, and caps. A project whose burst was all noise gets no
/// batch. Batches come out in project-id order.
pub fn coalesce(raws: &[Raw]) -> Vec<Batch> {
    let mut by_project: HashMap<u32, (BTreeSet<String>, bool)> = HashMap::new();
    for raw in raws {
        let (paths, rescan) = by_project.entry(raw.project).or_default();
        *rescan |= raw.rescan;
        for p in &raw.paths {
            if !is_noise(&raw.root, p) {
                paths.insert(p.to_string_lossy().into_owned());
            }
        }
    }
    let mut out: Vec<Batch> = by_project
        .into_iter()
        .filter(|(_, (paths, rescan))| *rescan || !paths.is_empty())
        .map(|(project_id, (paths, rescan))| Batch {
            project_id,
            paths: if rescan || paths.len() > CAP { None } else { Some(paths.into_iter().collect()) },
        })
        .collect();
    out.sort_by_key(|b| b.project_id);
    out
}

pub struct Watchers {
    inner: Mutex<HashMap<u32, (PathBuf, RecommendedWatcher)>>,
    tx: Sender<Raw>,
}

impl Watchers {
    /// Starts the coalescing thread; `sink` is called once per project per
    /// burst, from that thread.
    pub fn new(sink: impl Fn(Batch) + Send + 'static) -> Self {
        let (tx, rx) = channel::<Raw>();
        std::thread::Builder::new()
            .name("veal-watch".into())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    let started = Instant::now();
                    let mut burst = vec![first];
                    loop {
                        let elapsed = started.elapsed();
                        if elapsed >= MAX_LATENCY {
                            break;
                        }
                        match rx.recv_timeout(QUIET.min(MAX_LATENCY - elapsed)) {
                            Ok(raw) => burst.push(raw),
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    for batch in coalesce(&burst) {
                        sink(batch);
                    }
                }
            })
            .expect("spawn watch thread");
        Self { inner: Mutex::new(HashMap::new()), tx }
    }

    /// Watches `root` recursively for `project`, replacing any earlier watch
    /// for that id. The error is the OS's (a missing or unwatchable root).
    pub fn watch(&self, project: u32, root: &Path) -> Result<(), String> {
        let root = root.to_path_buf();
        let tx = self.tx.clone();
        let tag_root = root.clone();
        let handler = move |res: notify::Result<Event>| {
            let raw = match res {
                Ok(ev) => {
                    let rescan = ev.need_rescan();
                    Raw { project, root: tag_root.clone(), paths: ev.paths, rescan }
                }
                Err(_) => Raw { project, root: tag_root.clone(), paths: vec![], rescan: true },
            };
            let _ = tx.send(raw);
        };
        let mut watcher = RecommendedWatcher::new(handler, Config::default())
            .map_err(|e| format!("watch {}: {e}", root.display()))?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| format!("watch {}: {e}", root.display()))?;
        self.lock().insert(project, (root, watcher));
        Ok(())
    }

    /// Dropping the watcher closes the directory handle.
    pub fn unwatch(&self, project: u32) {
        self.lock().remove(&project);
    }

    #[cfg(test)]
    pub fn roots(&self) -> Vec<(u32, PathBuf)> {
        let mut v: Vec<_> = self.lock().iter().map(|(id, (root, _))| (*id, root.clone())).collect();
        v.sort_by_key(|(id, _)| *id);
        v
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u32, (PathBuf, RecommendedWatcher)>> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc::Receiver;

    fn raw(project: u32, root: &Path, paths: &[&str], rescan: bool) -> Raw {
        Raw { project, root: root.to_path_buf(), paths: paths.iter().map(|p| root.join(p)).collect(), rescan }
    }

    fn s(root: &Path, p: &str) -> String {
        root.join(p).to_string_lossy().into_owned()
    }

    #[test]
    fn coalesce_filters_dedups_and_sorts() {
        let root = Path::new(r"C:\p");
        let raws = [
            raw(2, root, &["b.txt"], false),
            raw(1, root, &[r".git\index.lock", "a.txt.veal-tmp", ".gitignore", "a.txt"], false),
            raw(1, root, &["a.txt", r".veal\tasks.json.tmp", "a.txt"], false),
        ];
        assert_eq!(
            coalesce(&raws),
            vec![
                Batch { project_id: 1, paths: Some(vec![s(root, ".gitignore"), s(root, "a.txt")]) },
                Batch { project_id: 2, paths: Some(vec![s(root, "b.txt")]) },
            ]
        );
    }

    #[test]
    fn coalesce_caps_and_rescans() {
        let root = Path::new(r"C:\p");
        let many: Vec<String> = (0..=CAP).map(|i| format!("f{i}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        assert_eq!(coalesce(&[raw(1, root, &refs, false)])[0].paths, None);
        let under: Vec<&str> = refs[..CAP].to_vec();
        assert_eq!(coalesce(&[raw(1, root, &under, false)])[0].paths.as_ref().map(Vec::len), Some(CAP));
        assert_eq!(coalesce(&[raw(1, root, &["a"], true)])[0].paths, None);
        // A rescan with no paths still produces a batch; all-noise does not.
        assert_eq!(coalesce(&[raw(3, root, &[], true)]), vec![Batch { project_id: 3, paths: None }]);
        assert_eq!(coalesce(&[raw(3, root, &[r".git\HEAD"], false)]), vec![]);
    }

    fn watchers() -> (Watchers, Receiver<Batch>) {
        let (tx, rx) = channel();
        (Watchers::new(move |b| { let _ = tx.send(b); }), rx)
    }

    #[test]
    fn watches_a_root_and_ignores_git() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        let (w, rx) = watchers();
        w.watch(1, &root).unwrap();
        assert_eq!(w.roots(), vec![(1, root.clone())]);

        fs::write(root.join("a.txt"), "a").unwrap();
        let b = rx.recv_timeout(Duration::from_secs(3)).expect("batch for a.txt");
        assert_eq!(b.project_id, 1);
        let paths = b.paths.expect("paths");
        assert!(paths.iter().any(|p| p.ends_with("a.txt")), "{paths:?}");

        fs::write(root.join(".git").join("x"), "x").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err(), "git traffic must be dropped");

        w.unwatch(1);
        assert!(w.roots().is_empty());
        fs::write(root.join("b.txt"), "b").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err(), "unwatched root must be silent");
    }

    #[test]
    fn a_burst_is_one_batch() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (w, rx) = watchers();
        w.watch(7, &root).unwrap();
        for i in 0..20 {
            fs::write(root.join(format!("f{i}.txt")), "x").unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        let b = rx.recv_timeout(Duration::from_secs(3)).expect("one batch");
        assert_eq!(b.project_id, 7);
        assert_eq!(b.paths.map(|p| p.len()), Some(20));
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err(), "no second batch");
    }

    #[test]
    fn missing_root_is_an_error() {
        let (w, _rx) = watchers();
        assert!(w.watch(1, Path::new(r"C:\definitely\not\here")).is_err());
    }
}
