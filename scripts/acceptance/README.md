# Acceptance scripts (plan-19)

Tools for running plan-19's acceptance list by hand or from a session. They are
not part of `pnpm test`.

**They act on whatever Veal they reach.** Run them against the test identity
(below) unless you mean to touch your own Veal and its `veal.db`. The daemon's
pipe is per user (`\.\pipe\veal-<SID>`), so close the real Veal first: a test
build and the real one would share a daemon.

## Test identity

`src-tauri/tauri.test.conf.json` builds "Veal Test" (`com.veal.test`), installed
to `%LOCALAPPDATA%\Veal Test` with its own data folder, and checks for updates
at `http://127.0.0.1:8765/latest.json`.

```bash
bash scripts/acceptance/build-test.sh <feed-dir> 0.1.92      # signed installer(s) into <feed-dir>
python scripts/acceptance/write-feed.py <feed-dir> 0.1.92     # latest.json for that version
python -m http.server 8765 --bind 127.0.0.1 --directory <feed-dir>
```

Install silently with `"Veal Test_0.1.92_x64-setup.exe" /S`.

## Driving the window

Start Veal with remote debugging on its webview:

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
& "$env:LOCALAPPDATA\Veal Test\veal.exe"
```

| Script | What it does |
|---|---|
| `cdp.mjs <port> eval <expr>` | Evaluates in the page, prints JSON |
| `cdp.mjs <port> type <pane> <text>` | Types into pane N (`\r` is not Enter; use `enter`) |
| `cdp.mjs <port> enter <pane>` | Presses Enter in pane N |
| `cdp.mjs <port> ctrlc <pane>` | Sends Ctrl+C to pane N |
| `cdp.mjs <port> pause <seconds>` | Freezes the page in the debugger for that long |
| `cdp.mjs <port> shot <file>` | Screenshot as PNG |
| `closewin.ps1 [exe-path]` | WM_CLOSE to Veal's window, as the X does; the path picks one `veal.exe` |
| `uia.ps1 list <pid>` | Named elements in a process's windows (NSIS, message boxes) |
| `uia.ps1 click <pid> <name>` | Clicks a button by name with `BM_CLICK` |
| `old-session.mjs <pipe>` | Starts a session speaking protocol 99, as an older Veal would |
| `rows.ps1 -Project <folder> [-Rows 3,4,...]` | Runs every scriptable row against the installed test build, one result line each (call it with `&`, not `-File`, to pass `-Rows`) |
| `fake-agent.ps1 <dir>` | Makes `<dir>\claude.exe` (a copy of `ping.exe`); `claude.exe -t 127.0.0.1` runs until killed and counts as an agent |
