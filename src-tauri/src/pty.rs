//! PTY plumbing: one real shell process per pane, driven by portable-pty.
//!
//! Design: no async runtime. portable-pty is blocking, so each pane owns two
//! `std::thread`s — a reader that pumps raw bytes to the frontend and a waiter
//! that blocks on `child.wait()`. The waiter is what tears the pane down: on
//! Windows the ConPTY reader only sees EOF once the pseudoconsole is closed,
//! which happens when we drop the master, so "child exited" has to be observed
//! independently of the byte stream.
//!
//! The registry is Tauri-agnostic (callbacks instead of channels/events) so it
//! can be exercised from `cargo test` without a webview.

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Bytes per `read()` on the PTY master. Each chunk goes straight to the
/// frontend as one channel message; no coalescing in v1.
const READ_BUF: usize = 16 * 1024;

pub struct SpawnOptions {
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    /// Explicit shell/program. `None` picks the platform default.
    pub shell: Option<String>,
}

struct PtyHandle {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default)]
pub struct PtyRegistry {
    next_id: AtomicU32,
    panes: Mutex<HashMap<u32, PtyHandle>>,
}

fn no_pty(id: u32) -> String {
    format!("no pty {id}")
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Platform default shell. Windows: `pwsh.exe` if it is on PATH, else
/// `powershell.exe`. Elsewhere: `$SHELL`, else `/bin/sh`.
pub fn default_shell() -> String {
    if cfg!(windows) {
        let on_path = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join("pwsh.exe").is_file()))
            .unwrap_or(false);
        if on_path {
            "pwsh.exe".into()
        } else {
            "powershell.exe".into()
        }
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }
}

impl PtyRegistry {
    /// Spawn a shell. `on_data` runs on the reader thread for every chunk;
    /// `on_exit` runs exactly once, after the handle has been removed from the
    /// registry, with the pane id and exit code (`None` if it could not be read).
    pub fn spawn(
        self: &Arc<Self>,
        opts: SpawnOptions,
        mut on_data: impl FnMut(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(u32, Option<i32>) + Send + 'static,
    ) -> Result<u32, String> {
        let pair = native_pty_system()
            .openpty(size(opts.cols, opts.rows))
            .map_err(|e| e.to_string())?;

        let shell = opts.shell.unwrap_or_else(default_shell);
        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(&opts.cwd);
        // Ignored by ConPTY; required on unix so the shell knows it has a real terminal.
        cmd.env("TERM", "xterm-256color");

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("spawn {shell}: {e}"))?;
        // The slave handles belong to the child now.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let killer = child.clone_killer();

        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.panes.lock().unwrap().insert(
            id,
            PtyHandle {
                master: pair.master,
                writer,
                killer,
            },
        );

        thread::Builder::new()
            .name(format!("pty-{id}-read"))
            .spawn(move || {
                let mut buf = vec![0u8; READ_BUF];
                let mut chunks = 0u32;
                let mut window = std::time::Instant::now();
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            on_data(buf[..n].to_vec());
                            if cfg!(debug_assertions) {
                                chunks += 1;
                                if window.elapsed().as_secs() >= 1 {
                                    eprintln!("[pty {id}] {chunks} chunks/s");
                                    chunks = 0;
                                    window = std::time::Instant::now();
                                }
                            }
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        let registry = Arc::clone(self);
        thread::Builder::new()
            .name(format!("pty-{id}-wait"))
            .spawn(move || {
                let code = child.wait().ok().map(|s| s.exit_code() as i32);
                // Drop the handle outside the lock: on Windows this is
                // ClosePseudoConsole, which blocks until output is drained.
                let handle = registry.panes.lock().unwrap().remove(&id);
                drop(handle);
                on_exit(id, code);
            })
            .map_err(|e| e.to_string())?;

        Ok(id)
    }

    pub fn write(&self, id: u32, data: &[u8]) -> Result<(), String> {
        let mut panes = self.panes.lock().unwrap();
        let h = panes.get_mut(&id).ok_or_else(|| no_pty(id))?;
        h.writer.write_all(data).map_err(|e| e.to_string())?;
        h.writer.flush().map_err(|e| e.to_string())
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let panes = self.panes.lock().unwrap();
        let h = panes.get(&id).ok_or_else(|| no_pty(id))?;
        h.master.resize(size(cols, rows)).map_err(|e| e.to_string())
    }

    /// Terminate the child. Idempotent: an unknown id is not an error, because
    /// the waiter thread may already have removed it. Cleanup and the exit
    /// callback still happen on the waiter thread.
    pub fn kill(&self, id: u32) -> Result<(), String> {
        let mut panes = self.panes.lock().unwrap();
        if let Some(h) = panes.get_mut(&id) {
            h.killer.kill().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Kill every registered child. Used on window close so no shells or
    /// agents outlive the app.
    pub fn kill_all(&self) {
        let mut panes = self.panes.lock().unwrap();
        for h in panes.values_mut() {
            let _ = h.killer.kill();
        }
    }

    #[cfg(test)]
    pub fn contains(&self, id: u32) -> bool {
        self.panes.lock().unwrap().contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn echo_shell() -> String {
        if cfg!(windows) {
            "cmd.exe".into()
        } else {
            "/bin/sh".into()
        }
    }

    /// Drain `rx` into `acc` until `pred` matches or `timeout` passes.
    fn read_until(
        rx: &mpsc::Receiver<Vec<u8>>,
        acc: &mut Vec<u8>,
        timeout: Duration,
        pred: impl Fn(&[u8]) -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        while !pred(acc) {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            match rx.recv_timeout(left) {
                Ok(chunk) => acc.extend(chunk),
                Err(_) => return false,
            }
        }
        true
    }

    /// Spawn a shell, type an echo, check the bytes come back through
    /// `on_data`, then that `on_exit` fires and the handle is gone.
    #[test]
    fn spawn_echo_reads_output_and_cleans_up() {
        let registry = Arc::new(PtyRegistry::default());
        let (data_tx, data_rx) = mpsc::channel::<Vec<u8>>();
        let (exit_tx, exit_rx) = mpsc::channel::<Option<i32>>();

        let id = registry
            .spawn(
                SpawnOptions {
                    cwd: std::env::temp_dir().to_string_lossy().into_owned(),
                    cols: 80,
                    rows: 24,
                    shell: Some(echo_shell()),
                },
                move |chunk| {
                    let _ = data_tx.send(chunk);
                },
                move |_id, code| {
                    let _ = exit_tx.send(code);
                },
            )
            .expect("spawn");
        assert!(registry.contains(id));

        let mut out = Vec::new();
        if cfg!(windows) {
            // ConPTY asks the terminal where the cursor is (CSI 6n) before it
            // runs the shell and blocks until it gets an answer. xterm.js
            // replies in the app; here we have to play the terminal.
            let asked = read_until(&data_rx, &mut out, Duration::from_secs(10), |b| {
                b.windows(4).any(|w| w == b"\x1b[6n")
            });
            assert!(
                asked,
                "no DSR from ConPTY; got: {:?}",
                String::from_utf8_lossy(&out)
            );
            registry.write(id, b"\x1b[1;1R").unwrap();
        }

        // Type only once the shell is at its prompt; input sent while it is
        // still starting up can be dropped.
        let prompted = read_until(&data_rx, &mut out, Duration::from_secs(10), |b| {
            b.contains(&b'>') || b.contains(&b'$')
        });
        assert!(
            prompted,
            "no prompt; got: {:?}",
            String::from_utf8_lossy(&out)
        );

        registry.write(id, b"echo veal-ok\r\n").unwrap();
        let echoed = read_until(&data_rx, &mut out, Duration::from_secs(10), |b| {
            // Look past the input echo for the command's own output line.
            String::from_utf8_lossy(b).matches("veal-ok").count() >= 2
        });
        assert!(
            echoed,
            "no echo output; got: {:?}",
            String::from_utf8_lossy(&out)
        );

        registry.write(id, b"exit\r\n").unwrap();
        let code = exit_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("pty-exit within 10s");
        assert_eq!(code, Some(0));
        assert!(!registry.contains(id), "handle removed after exit");
    }

    #[test]
    fn unknown_id_errors() {
        let registry = Arc::new(PtyRegistry::default());
        assert_eq!(registry.write(7, b"x"), Err("no pty 7".into()));
        assert_eq!(registry.resize(7, 80, 24), Err("no pty 7".into()));
        assert_eq!(registry.kill(7), Ok(()));
    }
}
