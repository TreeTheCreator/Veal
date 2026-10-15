# Veal — plan for step 19: Panes follow the system proxy

Status: build order 1–5 BUILT 2026-10-14, not yet committed. 6 (the
acceptance list) partly run, see below; 7 not started.

Acceptance (2026-10-14, dev window over CDP plus by hand): A1 passes. A
Follow-system pane had `https_proxy` `http://127.0.0.1:15236`, curl
worked through it, and `claude` started and drew its start screen. A2b
passes, run by the user: with "Use a proxy server" off and Veee still
connected, a new pane got no proxy and `claude` in it failed. Direct
`curl https://api.anthropic.com` from this machine answers 403 (blocked),
against 404 (normal) through Veee. So Follow system with the switch off
means no VPN for panes, as designed; Manual is the way to pin Veee. A9
passes: three curl runs each, Follow system connect 1.1–1.2 ms and total
1.02–1.04 s, Manual connect 1.0–1.6 ms and total 0.99–1.02 s. A10 passes
(below). A5 is covered by the store tests only. Run by the user: A2
passes, because disconnecting Veee turns "Use a proxy server" off and new
panes get no proxy. A3 passes on the variables: with ExpressVPN, no proxy
is exported (the curl code was not reported). A4 did not test its case.
Ending Veee in Task Manager also cleared its proxy, and restarting it put
15236 back, so the "proxy left set, nothing listening" path never
occurred. That path is covered only by `liveness_tries_ipv4_then_ipv6…`
and `tcp_probe_sees_a_listener_and_its_absence`. A7 and A8 were skipped:
neither of this user's VPNs uses a setup script. Their reporting is
covered by `system_script_only_autodetect_none_and_unavailable_give_none`
and `system_static_with_a_script_uses_the_static_and_says_so`. Which of
the two Windows prefers when both are set is still open. A dev-only trap found on the way: a debug `veal.exe` whose
`tauri dev` has exited loses stderr, and the debug-only `eprintln!` in
the pty reader thread (`[pty N] KB/s read`) then panics on the first read
more than a second after spawn. That kills the thread: the display freezes
while the shell keeps running. Release builds do not have that line. It
was seen once here, after a failed second `tauri dev`, and not reproduced
on purpose.

Results for 5: `pnpm test` 180, `cargo test` 83 (+2 ignored), `tsc`
clean, `pnpm build` green, no new clippy warnings. The proxy rows are
their own component, `ProxySettings.tsx`, in place of the port row:
Off / Manual / Follow system (port field only under Manual), Bypass with
a Default button, "New panes get" with Re-check, and a collapsed Details
(the exact variables, lookup type and time, the raw proxy, bypass and
script, host, which address answered, plus the localhost and
background-agent notes). Beyond the plan: `proxy_resolve` also takes
`proxy_bypass` and returns the pane's exact `env` next to `Resolved`, so
the Details panel shows the merged `no_proxy` without a second copy of
the merge in TS. Notes from Rust use plain quotes, not backticks (the UI
renders them as text). A `--warn` token (#e3b34f dark, #9a6a00 light) is
new. The check runs 250 ms after the last change, and only the latest
answer is shown. Dev window over CDP, Veee connected: the saved settings
had no port and migrated to Follow system. The row read `Follow system ·
http://127.0.0.1:15236`, Details `fixed proxy · 4 ms`, raw
`http://localhost:15236`, host `127.0.0.1 (from localhost)`, answered
`127.0.0.1`. Manual 15236 gave the same URL; a blank port gave "no port
set" and 99999 gave `"99999" is not a valid port`, both amber (checked
readable in light theme). Off and a blank Bypass exported none and no
`no_proxy` respectively; `localhost, .corp` became `no_proxy=localhost,.corp`.
A10 passes: a pane opened with + New terminal had `https_proxy` and
`http_proxy` `http://127.0.0.1:15236`, the default `no_proxy` and
`NODE_USE_ENV_PROXY=1`, equal to the row, and `curl.exe
https://api.anthropic.com` from it answered in 1.01 s. Settings, theme
and layout were put back afterwards. Not yet run: the rest of A1 (start
`claude` in a pane), A2, A2b in a pane (the API half passed in step 4),
A3, A4 (needs Veee killed), A7–A9, and A6 belongs to step 7.

Results for 4: `pnpm test` 177 → 180, `cargo test` 82 → 83 (+2 ignored:
the old `poller_cost` and the new `what_a_pane_gets`), `tsc` clean,
`pnpm build` green, no new clippy warnings. A throwaway program
(scratchpad, not in the repo) printed the raw output of
`WinHttpGetIEProxyConfigForCurrentUser` with Veee connected. Switch on:
`lpszProxy` `"http://localhost:15236"`, `fAutoDetect` 1, the script URL
and the bypass list NULL (the registry's empty `ProxyOverride` comes back
NULL, not `""`), 8.5–10.6 ms in a debug build. Switch off: `lpszProxy`
NULL while the registry's `ProxyServer` still held the address and
`ProxyEnable` was 0, and Veee was still listening. So A2b's API half
passes and no `ProxyEnable` read is needed. `fAutoDetect` is 1 alongside
the fixed proxy, and it stays on with the switch off, so the
auto-detect-only row is what nearly every user without a proxy hits. It
now gives no note, only a Details line (step 3 table updated).
`what_a_pane_gets` on this machine, with the real lookup and liveness:
`static`, raw `http://localhost:15236`, answered `127.0.0.1`, 7 ms, and the
env is `http_proxy` and `https_proxy` `http://127.0.0.1:15236`, the default
`no_proxy` and `NODE_USE_ENV_PROXY=1`. A mistyped `invoke` key (a
deliberate `proxy_bypass`) fails both payload tests. Against step 4:
`SpawnOptions` lost its `proxy` field after all. Every caller of `spawn`
other than `pty_spawn` is a test, so keeping the field and the wrapper
left three dead-code warnings in the shipped build. `spawn` is now a
`cfg(test)` shorthand for `spawn_with_env` with no env, and the five test
literals lost their `proxy: None` line. `proxy_is_exported_to_the_shell`
now goes `resolve` → `proxy_env` → `spawn_with_env`, the same path as
`pty_spawn`. The payload test reads the Rust parameter names from
`lib.rs` (`?raw` import) instead of a hardcoded list. A missing or unknown
`proxyMode` resolves as Follow system. `proxyForSpawn` is gone; Settings
still uses `parsePort`.

Results for 1–3: `pnpm test` 170 → 177, `cargo test` 59 → 82 (+1
ignored, unchanged), `tsc` clean, `pnpm build` green. Gate 0a: the
registry has `ProxyEnable` 1, `ProxyServer` `http://localhost:15236` (the
scheme is in the stored string), and `ProxyOverride` and `AutoConfigURL`
empty, so there is no setup script. Veee (pid 7212) listens on
`127.0.0.1:15236` only, not `::1`. Gate 0b, curl against
`https://api.anthropic.com` through the proxy, three runs each: connect
0.203–0.211 s through `localhost` and 0.002–0.004 s through `127.0.0.1`,
totals 1.21–1.28 s against 1.03–1.04 s. `curl -v` shows `[::1]` tried
first. Cause (b) is real, and the normalisation in step 5 saves ~200 ms
per connection. Found on the way, against step 3's liveness text: a
refused loopback connect on this machine takes ~2 s, not "instantly"
(`TcpClient` to a closed port: 2080 ms on `127.0.0.1` and 2028–2032 ms on
`::1`), so with the 200 ms timeout a dead proxy (A4) costs a new pane
~400 ms, two timeouts, and the live case costs one fast connect. Beyond
the plan: `proxy.rs` sits in `src-tauri/src` with inline tests, not a
scratch crate, because the crate builds on this machine. It is `mod`'d
with `allow(dead_code)` until step 4, which removed it. Liveness applies to Follow system
only. Manual exports its port unchecked, as today (A5). For SOCKS-only,
`url` holds the `socks5h://` URL rather than an `http://` one. A
per-protocol list with different `http=` and `https=` proxies uses the
`https=` one for both, with a note. A proxy string without a port is
rejected rather than defaulting to 80. `proxyForSpawn` stays until step 4
replaces the spawn arguments.

Gate (v3): this machine's Windows proxy is a fixed proxy, address
`http://localhost`, port `15236`, not a PAC script. That removes most of
v2's machinery, which is deferred (step 10).

Scope: new panes pick up the proxy Windows is using, with no port typed in.
Settings gets a mode: Off / Manual / Follow system, and Follow system is the
default. Manual is today's port field. Follow system reads Windows' proxy
setting and exports it to the pane's environment. Settings shows exactly
what a new pane would get, and why.

## Why it fails today

1. **The VPN sets a fixed system proxy.** Reported from Settings → Network &
   internet → Proxy: `http://localhost`, port `15236`. Browsers and
   Chromium-based apps use that setting. `curl`, `git`, `npm`, `pip`, Node
   and `claude` read only the proxy environment variables, and a shell
   started by Veal has none, so its traffic leaves directly and bypasses
   the VPN. ExpressVPN, as far as I know, captures traffic at the network
   adapter and sets no proxy, so nothing has to opt in (not checked from a
   source). This assumes "Use setup script" is off; step 0a confirms it.
2. **Why the port field works.** It exports the same proxy to the pane:
   `http://127.0.0.1:15236`. Veal's own tests already use 15236.
3. **Why it is "slow sometimes".** Not a per-host routing problem on Veal's
   side: a fixed Windows proxy sends everything through it for the browser
   too, so the manual field behaves like the browser does. The candidates
   now: (a) the VPN's proxy or server (load and distance; Veee's FAQ names
   both). (b) `localhost` itself. Windows can resolve `localhost` to `::1`
   first, and if the proxy listens only on IPv4, some tools stall or get a
   refused connection before falling back. Veal's field uses `127.0.0.1`, so
   it avoids this, and **Follow system must not export `localhost` as
   written** (step 5). Plausible, not verified: step 0b measures it. Veal
   cannot make the VPN's proxy faster, and this plan does not claim to.

## 0. Gate

Done: the proxy is `http://localhost` : `15236`.

a. Confirm that "Use a proxy server" is on and "Use setup script" is off.
   If a script is on as well, say so; step 3 covers it, and it is
   acceptance A8.
b. Timing test for the slowness, with Veee connected:
   `curl.exe -s -o NUL -w "%{time_total}\n" --proxy http://localhost:15236
   https://api.anthropic.com` against the same with `--proxy
   http://127.0.0.1:15236`. Run each three times. A consistent gap means
   cause (b) is real and the normalisation in step 5 is doing real work.
c. Optional, for understanding only: in VS Code's integrated terminal (not
   the extension), `echo $env:HTTPS_PROXY`. The plan does not depend on why
   the Claude Code extension works there.

| Gate result | Consequence |
| --- | --- |
| Script on, no fixed proxy | This is the deferred PAC case (step 10). Build Manual and the mode setting only. |
| Proxy is SOCKS only | Claude Code does not support SOCKS (its docs say so), so this plan cannot fix `claude`. |
| `localhost` and `127.0.0.1` differ in speed | Keep the normalisation and say why in Settings. |

## 1. Pure module: `src-tauri/src/proxy.rs` (no Tauri, no Windows)

Every decision lives here so it can be tested on Linux:

- `parse_static(s)`: `host:port`, `http://host:port`, `[::1]:port`, and
  `http=h:p;https=h:p;socks=h:p`.
- `normalise_host`: `localhost` becomes `127.0.0.1` (step 5).
- `resolve(mode, manual, lookup) -> Resolved`: the decision table in step 3.
- `proxy_env(&Resolved, bypass) -> Vec<(String, String)>` (step 5).
- `normalise_bypass`, `is_loopback_host`.

`Resolved { source, url, socks_only, notes, diag }`. `url` is always a full
`http://host:port`.

## 2. Thin Windows layer: `src-tauri/src/sysproxy.rs`

`cfg(windows)`, with a stub elsewhere that returns `Lookup::Unavailable`, so
`pnpm tauri dev` and the unix test paths still compile. One API call, about
40 lines of FFI.

`windows-sys` 0.61.2 is already pinned. Features to add:
`Win32_Networking_WinHttp` and `Win32_Foundation` (also holds `GetLastError`
and `GlobalFree`; checked in the crate, which also has
`WinHttpGetIEProxyConfigForCurrentUser`). No new crate, no registry code, no
network calls.

Flow:

1. `WinHttpGetIEProxyConfigForCurrentUser`. On FALSE, `GetLastError`:
   `ERROR_FILE_NOT_FOUND` means no settings (treat as none), anything else
   becomes `Lookup::Unavailable(code)` and shows in Details.
2. Read `lpszProxy`, `lpszProxyBypass`, `lpszAutoConfigUrl`, `fAutoDetect`,
   then `GlobalFree` every non-null string (Microsoft's documented rule).
3. `lpszProxy` non-null → `Lookup::Static { proxy, bypass }`.
4. Else `lpszAutoConfigUrl` non-null → `Lookup::ScriptOnly { url }`.
5. Else `fAutoDetect` only → `Lookup::AutoDetectOnly`. No WPAD discovery:
   "Automatically detect settings" is on by default in Windows and would
   otherwise run for nearly every user.
6. Else `Lookup::None`.

Facts from Microsoft's documentation, and what they leave open:

- The call returns the settings of the **active connection** (LAN, dial-up
  or VPN), and must not be called from a service that does not impersonate
  a user. Veal is a user app, so this holds.
- `lpszProxy` is documented as set when "use a proxy server" is enabled. The
  page does not say it is NULL when that switch is off. Acceptance A2b tests
  it. If a stale value comes back, add a read of `ProxyEnable`
  (`Win32_System_Registry`) and trust it over the API.
- The docs do not say whether the call touches the network. It is a
  configuration read and should not; Details shows its duration (step 7).

## 3. Resolution rules (no fallthrough between modes)

Mode decides. Manual never falls back to system, and system never reads the
manual port.

| Mode | Lookup | Result |
| --- | --- | --- |
| off | any | none |
| manual | any | valid port → `http://127.0.0.1:port`; blank or bad → none |
| system | static, HTTP entry | use it |
| system | static and a script both set | use the static proxy, note "a setup script is also set" (which one Windows prefers is open: A8) |
| system | static, SOCKS only | `socks_only` (step 5) |
| system | script only | none, note "this proxy is set by a script (PAC); Follow system cannot read scripts yet, use Manual" |
| system | auto-detect only | none, no note (Windows' default; Details shows it) |
| system | none | none |
| system | unavailable | none, note with the error code |

Liveness: when the chosen host is loopback (`localhost`, `127.0.0.0/8`,
`::1`), TCP-connect with a 200 ms timeout, `127.0.0.1` first, then `::1`.
Export whichever answered. If neither does, the result is none with "not
reachable". A refused loopback connection fails instantly, so this costs
nothing in the normal case. The check is skipped for remote proxies, where
a short timeout would wrongly drop a slow corporate proxy.

## 4. Spawn path

`SpawnOptions` is unchanged. It is built literally at `lib.rs:64`,
`agents.rs:348` and `pty.rs:533, 616, 638, 674`; changing its shape breaks
all of them for nothing.

- `PtyRegistry::spawn_with_env(opts, env, on_data_ready, on_exit)` is the
  real spawn. `spawn(opts, …)` becomes a thin wrapper that derives its env
  from `opts.proxy` with the default bypass list, so every existing test
  literal compiles and behaves as before.
- `pty_spawn` resolves the proxy, builds the env with `proxy_env`, and calls
  `spawn_with_env`. Its new arguments are `proxy_mode`, `proxy_port` and
  `proxy_bypass` (all `Option<…>`, camelCase from JS). The old `proxy`
  argument goes.
- A `proxy_resolve` command with the same arguments returns `Resolved` to
  Settings, registered in `invoke_handler`. One code path serves both, so
  Settings shows what a pane gets.
- A mistyped key would silently become `None`, so: a TS test that the
  `invoke` payload from `spawnPty` contains exactly those keys, and
  acceptance A10 (Settings row equals `$env:https_proxy`).

## 5. What goes into the environment

- **Always a full URL**: `http://host:port`. The Windows string may be a
  bare `host:port`; Claude Code's docs say it stops launching, with an error
  naming the variable, when the proxy URL cannot be parsed (a missing
  `http://` is their example). Test: no value from `proxy_env` is
  scheme-less.
- **`localhost` is exported as `127.0.0.1`** (or `[::1]` if only that
  answered the liveness check), never as written. This is the one place
  Follow system could behave worse than your manual field; see the
  slowness cause (b).
- `http_proxy` and `https_proxy` both get that URL (an HTTP proxy serves
  CONNECT). Lowercase only: portable-pty lowercases env keys on Windows, so
  a second casing replaces the first, and Claude Code reads `https_proxy`
  first anyway.
- `no_proxy` comes from the bypass setting (step 7), default
  `localhost,127.0.0.1,::1,.local`. Claude Code accepts comma or space
  separated. Windows' bypass list (`ProxyOverride`) is translated
  conservatively: exact hosts and plain IPs kept, `*.x` becomes `.x`,
  `<local>` is covered by the defaults, everything else is dropped, and
  Settings says how many entries were dropped. Private ranges are not added
  by default: support for CIDR and wildcards differs by tool and I have not
  checked which ones accept them.
- `NODE_USE_ENV_PROXY=1`, as now.
- **SOCKS-only**: set `ALL_PROXY=socks5h://host:port` for curl and git, set
  no `http(s)_proxy`, and show a red state: "Claude Code does not support
  SOCKS proxies". Never present this as working.

## 6. Latency

No cache, no wait budget, no extra threads. The lookup is a local
configuration read, and the only wait is the loopback liveness check, which
is instant on refusal and 200 ms at worst. (v2 had a single-flight cache for
slow PAC lookups; with no script involved it is not needed.) If Details
shows the lookup taking more than about 50 ms in the acceptance runs, add a
short memo then.

## 7. Settings UI

`SettingsView.tsx`: replace the Proxy port row with:

- A three-way control: Off / Manual / Follow system. Manual shows the port.
- A Bypass field (comma list) in every mode, with a hint that wildcard and
  range support varies by tool.
- A read-only row: `source · url`, plus the notes and states from step 3,
  with an amber or red state when the proxy is unreachable, SOCKS-only, or
  script-only.
- A Details section: lookup type, the raw proxy and bypass strings as
  Windows gave them, the host after normalisation, which address answered
  the liveness check, lookup duration, and any error code. The Windows call
  can only be exercised on the Windows machine, so this panel is how it gets
  debugged.
- Fixed notes: applies to new panes only; a running `claude` keeps the
  environment it started with; Claude's background agents use a supervisor
  that outlives the shell and takes the environment of whichever shell
  started it first, so for those put the proxy in the `env` block of
  `~/.claude/settings.json` (Veal does not edit that file).

## 8. Settings keys and migration

New keys `proxy_mode` and `proxy_bypass`; `proxy_port` stays. `db.rs` is a
plain key/value table with no key whitelist (checked), so there is no Rust
DB change.

`load()`: a missing `proxy_mode` with a valid `proxy_port` loads as
`manual`, otherwise `system`. Persist the migrated value once, so clearing
the port later does not flip someone's mode. Default for new installs is
`system`, because the goal is that people without a port just work: with no
system proxy it produces no proxy variables, so full-tunnel VPN users are
untouched. Accepted risk: a corporate proxy setting now governs Veal's
panes the way it governs the browser; Off is one click.

`store.test.ts` asserts today's defaults and needs the new ones plus
migration cases (saved port, no port, explicit mode, clearing the port after
migration).

## 9. WSL, last and gated

Only when the pane shell is `wsl.exe`.

- Read the inherited `WSLENV` and append the proxy names with `/u`.
  `CommandBuilder::env` overwrites rather than appends, so setting it
  blindly would destroy the user's existing forwarding.
- Forward only when WSL networking is mirrored (read `networkingMode` under
  `[wsl2]` in `%UserProfile%\.wslconfig`; no file means NAT) or the proxy
  host is not loopback. In NAT mode `127.0.0.1` is WSL's own loopback, so
  forwarding it would break WSL networking that works today.
- Otherwise forward nothing and say so in Settings.
- A `wsl` typed inside a pwsh pane gets nothing in this version.

## 10. Deferred: script (PAC) support

Not built until someone actually has a script-only VPN. The v2 design is
kept here so the work is not lost:

- Per probe URL (`https://api.anthropic.com`, `https://github.com`,
  `https://registry.npmjs.org`), `WinHttpGetProxyForUrl` with
  `WINHTTP_AUTOPROXY_CONFIG_URL` only (no WPAD), after `WinHttpOpen`
  with `WINHTTP_ACCESS_TYPE_NO_PROXY`. Error codes to surface, all in the
  crate: 12166 bad script, 12167 download failed, 12178 auto-proxy service
  error, 12180 auto-detection failed.
- Fallback: for an `http://` loopback script URL only, fetch the text over a
  plain `TcpStream` (500 ms, 1 MB cap) and take the most frequent
  `PROXY` / `HTTPS` token, labelled "heuristic".
- Failure modes found in review that this design must handle: WinHTTP may
  not read a script that uses modern JavaScript; a Split-mode script can
  send every probe host DIRECT, which needs an opt-in "use the script's
  proxy anyway"; a hung lookup needs a single-flight cache so at most one
  thread is ever stuck; and SOCKS is unsupported by Claude Code.
- A per-URL forwarder (Veal listens on loopback and chains per the script)
  would be the only way to honour Split-mode routing for CLIs. Dropped for
  this user: with a fixed proxy there is no per-URL decision to follow, and
  the slowness is the proxy's, not Veal's.

## Build order

1. Gate 0a and 0b.
2. `proxy.rs` and its tests, in a scratch crate here.
3. `store.ts` migration and mode, with tests.
4. `sysproxy.rs`, then `spawn_with_env` and the commands.
5. Settings UI with the Details panel.
6. Windows acceptance checklist below.
7. WSL (step 9), only after 6 passes.

## Tests

Linux, in a scratch crate that pulls `proxy.rs` in with `#[path]`:

- `parse_static`: `localhost:15236`, `http://localhost:15236`,
  `127.0.0.1:15236`, `[::1]:15236`, `http=localhost:15236;https=…`, a
  SOCKS-only string, and garbage and out-of-range ports.
- `normalise_host`: `localhost` → `127.0.0.1`, case variants, and that
  non-loopback hosts are untouched.
- The step 3 table: every row.
- `proxy_env`: no scheme-less value; no `localhost` in any value; lowercase
  keys; SOCKS-only sets `ALL_PROXY` and no `http(s)_proxy`; default and
  custom bypass.
- `normalise_bypass` over `ProxyOverride`-style strings (`<local>`,
  `*.corp`, `192.168.*`, exact hosts), and `is_loopback_host`.
- TS: migration cases, and the `invoke` payload keys.

Why not in the `veal` crate: it does not build in this sandbox (no
`webkit2gtk`), and the Windows Rust target cannot be downloaded here (the
network blocks it; both checked). So `sysproxy.rs` is never even
type-checked off Windows. That is why it stays about 40 lines and why the
Details panel exists.

Windows, by hand:

- A1. Veee on, Follow system, new pane: `$env:https_proxy` is
  `http://127.0.0.1:15236`; `claude` starts without a proxy-URL error;
  `curl.exe -I https://api.anthropic.com` works.
- A2. Veee disconnected, new pane: no proxy variables.
- A2b. With Veee connected, switch "Use a proxy server" off in Windows
  Settings, then open a new pane: no proxy variables. This tests whether the
  API honours the switch (step 2).
- A3. ExpressVPN, Follow system: no proxy variables, everything works.
- A4. Kill Veee so it leaves the proxy set: the liveness check fails, no
  proxy variables, Settings says not reachable.
- A5. Upgrade with a saved port and no mode: loads as Manual, same env as
  before.
- A6. `wsl` shell in NAT mode: nothing forwarded, WSL networking unchanged.
- A7. Remove the fixed proxy and set only a setup script: no proxy
  variables, the script-only note shows, no error.
- A8. Fixed proxy and a setup script both set: the static proxy is used and
  the note shows. Record which one Windows itself prefers.
- A9. Speed: a pane under Follow system and one under Manual give the same
  timings against the same URL, and the `localhost` vs `127.0.0.1` result
  from gate 0b is reflected in Settings' wording.
- A10. The Settings row equals what `$env:https_proxy` shows in a new pane.

## Known unknowns

- ~~Whether `WinHttpGetIEProxyConfigForCurrentUser` returns NULL for
  `lpszProxy` when "Use a proxy server" is off (A2b).~~ It does (step 4
  results).
- Which of a script and a fixed proxy Windows prefers when both are set
  (A8).
- ~~Whether `localhost` really costs time in the tools you run (gate
  0b).~~ ~200 ms per connect for curl (step 1–3 results); other tools
  untested.
- Which tools accept CIDR or wildcards in `no_proxy`.
- Why the Claude Code extension works in VS Code (gate 0c). The plan does
  not rely on it.

## Revision log

v1 → v2: `GlobalFree` feature corrected to `Win32_Foundation`; "Manual
wins" replaced by no-fallthrough modes; default changed to Follow system;
proxy URLs always carry `http://`; WPAD auto-detect dropped; the VS Code
premise became a test; PAC, force option, single-flight cache and the
latency design added.

v2 → v3 (this version): gate result is a fixed proxy, so the PAC probes,
script heuristic, force option and single-flight cache moved to step 10 as
deferred; the Windows layer shrank to one local API call; `localhost` is
normalised to `127.0.0.1` and liveness tries both loopback addresses; a test
for whether the API honours the "Use a proxy server" switch was added
(A2b); the slowness section no longer promises Veal can fix the VPN's own
speed.
