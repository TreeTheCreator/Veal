// Which proxy a new pane gets, and the environment that carries it
// (plan-18). Every decision lives here, with no Tauri and no Windows calls:
// the Windows lookup (sysproxy.rs) and the TCP liveness probe come in as
// closures, so the whole table is testable anywhere.
use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

pub const DEFAULT_BYPASS: &str = "localhost,127.0.0.1,::1,.local";

/// Per address. A refused loopback connect is not instant on Windows (it
/// retries for ~2 s), so a dead proxy costs one timeout per address tried.
pub const LIVENESS_TIMEOUT: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Off,
    Manual,
    System,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "off" => Some(Mode::Off),
            "manual" => Some(Mode::Manual),
            "system" => Some(Mode::System),
            _ => None,
        }
    }
}

/// What Windows reported for the current user (sysproxy.rs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    None,
    /// "Use a proxy server" is set. `script` is the setup script, if one is
    /// set as well.
    Static { proxy: String, bypass: Option<String>, script: Option<String> },
    ScriptOnly { url: String },
    AutoDetectOnly,
    /// The call failed; the Win32 error code.
    Unavailable(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// Without brackets for IPv6.
    pub host: String,
    pub port: u16,
}

impl Endpoint {
    fn authority(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub endpoint: Endpoint,
    /// No HTTP entry, only a SOCKS one.
    pub socks_only: bool,
    pub note: Option<String>,
}

/// Windows' proxy string: `host:port`, `http://host:port`, `[::1]:port`, or
/// a per-protocol list such as `http=h:p;https=h:p;socks=h:p`. An HTTP proxy
/// wins over SOCKS; among HTTP entries `https=` wins, then `http=`, then an
/// untagged one.
pub fn parse_static(s: &str) -> Result<Parsed, String> {
    let mut https = None;
    let mut http = None;
    let mut untagged = None;
    let mut socks = None;
    for entry in s.split(';').map(str::trim).filter(|e| !e.is_empty()) {
        // `http=http://h:p` has its `=` before any `://`.
        let (key, value) = match entry.split_once('=') {
            Some((k, v)) if !k.contains("://") => (Some(k.trim().to_ascii_lowercase()), v.trim()),
            _ => (None, entry),
        };
        let (scheme, endpoint) = parse_endpoint(value)?;
        let is_socks = matches!(scheme.as_deref(), Some("socks" | "socks4" | "socks4a" | "socks5" | "socks5h"));
        let is_http = matches!(scheme.as_deref(), None | Some("http"));
        let slot = match key.as_deref() {
            Some("https") if is_http => &mut https,
            Some("http") if is_http => &mut http,
            Some("socks") if is_socks || scheme.is_none() => &mut socks,
            None if is_http => &mut untagged,
            None if is_socks => &mut socks,
            Some("ftp") => continue,
            _ => return Err(format!("unsupported entry \"{entry}\"")),
        };
        slot.get_or_insert(endpoint);
    }
    let note = match (&https, &http) {
        (Some(a), Some(b)) if a != b => Some(format!(
            "Windows has different HTTP ({}) and HTTPS ({}) proxies; using the HTTPS one for both",
            b.authority(),
            a.authority()
        )),
        _ => None,
    };
    if let Some(endpoint) = https.or(http).or(untagged) {
        return Ok(Parsed { endpoint, socks_only: false, note });
    }
    match socks {
        Some(endpoint) => Ok(Parsed { endpoint, socks_only: true, note: None }),
        None => Err(format!("no HTTP or SOCKS proxy in \"{s}\"")),
    }
}

/// `[scheme://]host:port[/]`, the port required.
fn parse_endpoint(s: &str) -> Result<(Option<String>, Endpoint), String> {
    let bad = |why: &str| format!("\"{s}\": {why}");
    let trimmed = s.trim().trim_end_matches('/');
    let (scheme, rest) = match trimmed.split_once("://") {
        Some((sc, r)) => (Some(sc.to_ascii_lowercase()), r),
        None => (None, trimmed),
    };
    let (host, port) = if let Some(r) = rest.strip_prefix('[') {
        let (h, after) = r.split_once(']').ok_or_else(|| bad("unclosed `[`"))?;
        h.parse::<Ipv6Addr>().map_err(|_| bad("not an IPv6 address"))?;
        (h, after.strip_prefix(':').ok_or_else(|| bad("no port"))?)
    } else {
        let (h, p) = rest.rsplit_once(':').ok_or_else(|| bad("no port"))?;
        let ok = !h.is_empty() && h.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c));
        if !ok {
            return Err(bad("not a host name"));
        }
        (h, p)
    };
    // `u16::from_str` accepts a leading `+`.
    let port = match port.parse::<u16>() {
        Ok(n) if n != 0 && port.bytes().all(|b| b.is_ascii_digit()) => n,
        _ => return Err(bad("port is not 1–65535")),
    };
    Ok((scheme, Endpoint { host: host.to_string(), port }))
}

/// `localhost` is exported as `127.0.0.1`: Windows tries `::1` first, and a
/// proxy listening on IPv4 only then costs every connection a fallback
/// (~200 ms per connect for curl on this machine, gate 0b).
pub fn normalise_host(host: &str) -> String {
    if host.eq_ignore_ascii_case("localhost") {
        "127.0.0.1".into()
    } else {
        host.to_string()
    }
}

pub fn is_loopback_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    h.eq_ignore_ascii_case("localhost") || h.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Windows' `ProxyOverride` as `no_proxy` entries, conservatively: exact
/// hosts and plain IPs are kept, `*.x` becomes `.x`, `<local>` is covered by
/// the defaults, anything else is dropped. Returns the kept entries and how
/// many were dropped.
pub fn normalise_bypass(windows: &str) -> (Vec<String>, usize) {
    let mut kept: Vec<String> = Vec::new();
    let mut dropped = 0;
    for entry in windows.split([';', ',', ' ', '\t']).map(str::trim).filter(|e| !e.is_empty()) {
        if entry.eq_ignore_ascii_case("<local>") {
            continue;
        }
        let candidate = entry.strip_prefix("*.").map(|d| format!(".{d}")).unwrap_or_else(|| entry.to_string());
        let body = candidate.strip_prefix('.').unwrap_or(&candidate);
        let plain = !body.is_empty()
            && (body.parse::<IpAddr>().is_ok()
                || body.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c)));
        if plain {
            let lower = candidate.to_ascii_lowercase();
            if !kept.contains(&lower) {
                kept.push(lower);
            }
        } else {
            dropped += 1;
        }
    }
    (kept, dropped)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Off,
    Manual,
    System,
}

/// What the Settings Details section shows: the lookup as Windows gave it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diag {
    /// `static`, `script`, `autodetect`, `none` or `unavailable`; `None`
    /// when the mode never looked.
    pub lookup: Option<String>,
    pub raw_proxy: Option<String>,
    pub raw_bypass: Option<String>,
    pub script_url: Option<String>,
    /// The proxy host after normalisation.
    pub host: Option<String>,
    /// The loopback address that answered the liveness check.
    pub answered: Option<String>,
    pub lookup_ms: Option<u64>,
    pub error_code: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    pub source: Source,
    /// `http://host:port`, or `socks5h://host:port` when `socks_only`.
    /// `None` exports no proxy.
    pub url: Option<String>,
    pub socks_only: bool,
    /// Kept entries of Windows' bypass list, added to `no_proxy`.
    pub bypass: Vec<String>,
    pub notes: Vec<String>,
    pub diag: Diag,
}

impl Resolved {
    fn none(source: Source) -> Resolved {
        Resolved { source, url: None, socks_only: false, bypass: Vec::new(), notes: Vec::new(), diag: Diag::default() }
    }
}

/// The decision table (plan-18 step 3). Mode decides, with no fallthrough:
/// Manual never looks at Windows, System never reads the manual port.
/// `lookup` is only called for System; `probe` only for a loopback system
/// proxy.
pub fn resolve(
    mode: Mode,
    manual_port: Option<&str>,
    lookup: impl FnOnce() -> Lookup,
    probe: impl Fn(SocketAddr) -> bool,
) -> Resolved {
    match mode {
        Mode::Off => Resolved::none(Source::Off),
        Mode::Manual => {
            let mut r = Resolved::none(Source::Manual);
            match manual_port.map(str::trim).filter(|p| !p.is_empty()) {
                Some(p) => match p.parse::<u16>() {
                    Ok(n) if n != 0 && p.bytes().all(|b| b.is_ascii_digit()) => {
                        r.url = Some(format!("http://127.0.0.1:{n}"));
                        r.diag.host = Some("127.0.0.1".into());
                    }
                    _ => r.notes.push(format!("\"{p}\" is not a valid port (1–65535)")),
                },
                None => r.notes.push("no port set".into()),
            }
            r
        }
        Mode::System => {
            let started = Instant::now();
            let found = lookup();
            let mut r = Resolved::none(Source::System);
            r.diag.lookup_ms = Some(started.elapsed().as_millis() as u64);
            resolve_system(&mut r, found, probe);
            r
        }
    }
}

fn resolve_system(r: &mut Resolved, found: Lookup, probe: impl Fn(SocketAddr) -> bool) {
    let (proxy, bypass, script) = match found {
        Lookup::None => {
            r.diag.lookup = Some("none".into());
            return;
        }
        Lookup::ScriptOnly { url } => {
            r.diag.lookup = Some("script".into());
            r.diag.script_url = Some(url);
            r.notes.push(
                "This proxy is set by a script (PAC); Follow system cannot read scripts yet, use Manual".into(),
            );
            return;
        }
        // "Automatically detect settings" is on by default in Windows, so
        // this is what nearly everyone without a proxy gets: no note, only
        // the Details line. WPAD discovery is not attempted.
        Lookup::AutoDetectOnly => {
            r.diag.lookup = Some("autodetect".into());
            return;
        }
        Lookup::Unavailable(code) => {
            r.diag.lookup = Some("unavailable".into());
            r.diag.error_code = Some(code);
            r.notes.push(format!("Could not read Windows' proxy setting (error {code})"));
            return;
        }
        Lookup::Static { proxy, bypass, script } => (proxy, bypass, script),
    };
    r.diag.lookup = Some("static".into());
    r.diag.raw_proxy = Some(proxy.clone());
    r.diag.raw_bypass = bypass.clone();
    r.diag.script_url = script.clone();

    let parsed = match parse_static(&proxy) {
        Ok(p) => p,
        Err(e) => {
            r.notes.push(format!("Could not read Windows' proxy: {e}"));
            return;
        }
    };
    if let Some(note) = parsed.note {
        r.notes.push(note);
    }
    if script.is_some() {
        r.notes.push("A setup script is also set; using the fixed proxy".into());
    }
    if let Some(b) = bypass.as_deref() {
        let (kept, dropped) = normalise_bypass(b);
        r.bypass = kept;
        if dropped > 0 {
            r.notes.push(format!(
                "{dropped} of Windows' bypass entries dropped (wildcards and ranges are not portable)"
            ));
        }
    }

    let mut host = normalise_host(&parsed.endpoint.host);
    if is_loopback_host(&host) {
        match alive(&host, parsed.endpoint.port, &probe) {
            Some(ip) => host = ip.to_string(),
            None => {
                r.diag.host = Some(host);
                r.notes.push(format!("Proxy {} is not reachable", parsed.endpoint.authority()));
                return;
            }
        }
        r.diag.answered = Some(host.clone());
    }
    let endpoint = Endpoint { host: host.clone(), port: parsed.endpoint.port };
    r.diag.host = Some(host);
    r.socks_only = parsed.socks_only;
    if parsed.socks_only {
        r.url = Some(format!("socks5h://{}", endpoint.authority()));
        r.notes.push("Claude Code does not support SOCKS proxies".into());
    } else {
        r.url = Some(format!("http://{}", endpoint.authority()));
    }
}

/// The loopback address that accepts a connection: the configured one
/// first, then the other family. `127.0.0.1` comes first for `localhost`.
fn alive(host: &str, port: u16, probe: &impl Fn(SocketAddr) -> bool) -> Option<IpAddr> {
    let v4: IpAddr = Ipv4Addr::LOCALHOST.into();
    let v6: IpAddr = Ipv6Addr::LOCALHOST.into();
    let candidates = match host.parse::<IpAddr>() {
        Ok(ip @ IpAddr::V4(_)) => [ip, v6],
        Ok(ip @ IpAddr::V6(_)) => [ip, v4],
        Err(_) => [v4, v6],
    };
    candidates.into_iter().find(|ip| probe(SocketAddr::new(*ip, port)))
}

/// The real liveness probe.
pub fn tcp_probe(addr: SocketAddr) -> bool {
    TcpStream::connect_timeout(&addr, LIVENESS_TIMEOUT).is_ok()
}

/// The pane's proxy variables. Keys are lowercase: portable-pty lowercases
/// env keys on Windows, so a second casing would only replace the first.
/// `bypass` is the user's Bypass setting (`None` for the default); Windows'
/// own kept entries are appended.
pub fn proxy_env(r: &Resolved, bypass: Option<&str>) -> Vec<(String, String)> {
    let Some(url) = r.url.as_deref() else { return Vec::new() };
    let mut no_proxy: Vec<String> = Vec::new();
    let user = bypass.unwrap_or(DEFAULT_BYPASS);
    for e in user.split([',', ';', ' ', '\t']).map(str::trim).filter(|e| !e.is_empty()).chain(r.bypass.iter().map(String::as_str)) {
        if !no_proxy.iter().any(|k| k.eq_ignore_ascii_case(e)) {
            no_proxy.push(e.to_string());
        }
    }
    let mut env = Vec::new();
    if r.socks_only {
        // curl and git read all_proxy; Claude Code cannot use SOCKS at all.
        env.push(("all_proxy".into(), url.to_string()));
    } else {
        env.push(("http_proxy".into(), url.to_string()));
        env.push(("https_proxy".into(), url.to_string()));
    }
    if !no_proxy.is_empty() {
        env.push(("no_proxy".into(), no_proxy.join(",")));
    }
    if !r.socks_only {
        // Node's built-in fetch ignores the variables without this (Node 24+).
        env.push(("NODE_USE_ENV_PROXY".into(), "1".into()));
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(host: &str, port: u16) -> Endpoint {
        Endpoint { host: host.into(), port }
    }
    fn http(host: &str, port: u16) -> Parsed {
        Parsed { endpoint: ep(host, port), socks_only: false, note: None }
    }
    fn stat(proxy: &str) -> Lookup {
        Lookup::Static { proxy: proxy.into(), bypass: None, script: None }
    }
    fn all_up(_: SocketAddr) -> bool {
        true
    }
    fn sys(lookup: Lookup) -> Resolved {
        resolve(Mode::System, Some("9999"), || lookup, all_up)
    }

    #[test]
    fn parses_the_single_proxy_forms() {
        assert_eq!(parse_static("localhost:15236"), Ok(http("localhost", 15236)));
        assert_eq!(parse_static("http://localhost:15236"), Ok(http("localhost", 15236)));
        assert_eq!(parse_static("HTTP://localhost:15236/"), Ok(http("localhost", 15236)));
        assert_eq!(parse_static("127.0.0.1:15236"), Ok(http("127.0.0.1", 15236)));
        assert_eq!(parse_static("[::1]:15236"), Ok(http("::1", 15236)));
        assert_eq!(parse_static("  proxy.corp.example:8080 "), Ok(http("proxy.corp.example", 8080)));
    }

    #[test]
    fn parses_per_protocol_lists() {
        assert_eq!(parse_static("http=localhost:15236;https=localhost:15236"), Ok(http("localhost", 15236)));
        assert_eq!(parse_static("http=http://a:1;socks=b:2"), Ok(http("a", 1)));
        assert_eq!(parse_static("socks=b:2;http=a:1"), Ok(http("a", 1)));
        assert_eq!(parse_static("ftp=f:21;http=a:1"), Ok(http("a", 1)));
        let differ = parse_static("http=a:1;https=b:2").unwrap();
        assert_eq!(differ.endpoint, ep("b", 2));
        assert!(differ.note.unwrap().contains("different"));
    }

    #[test]
    fn socks_only_is_flagged() {
        for s in ["socks=127.0.0.1:1080", "socks5://127.0.0.1:1080", "socks=socks5://127.0.0.1:1080;ftp=f:21"] {
            assert_eq!(
                parse_static(s),
                Ok(Parsed { endpoint: ep("127.0.0.1", 1080), socks_only: true, note: None }),
                "{s}"
            );
        }
    }

    #[test]
    fn rejects_garbage_and_bad_ports() {
        for s in [
            "", ";", "localhost", "localhost:", ":8080", "host:0", "host:65536", "host:99999", "host:+80",
            "host:abc", "host:-1", "[::1", "[nope]:80", "::1:80", "ftp=f:21", "https://h:443", "http=socks5://h:1",
            "user@host:80", "a b:80", "gopher=h:1",
        ] {
            assert!(parse_static(s).is_err(), "accepted \"{s}\"");
        }
        assert_eq!(parse_static("host:65535"), Ok(http("host", 65535)));
        assert_eq!(parse_static("host:1"), Ok(http("host", 1)));
    }

    #[test]
    fn localhost_is_normalised_and_nothing_else() {
        for h in ["localhost", "LOCALHOST", "LocalHost"] {
            assert_eq!(normalise_host(h), "127.0.0.1");
        }
        for h in ["127.0.0.2", "::1", "proxy.corp", "localhost.corp", "mylocalhost"] {
            assert_eq!(normalise_host(h), h);
        }
    }

    #[test]
    fn loopback_hosts() {
        for h in ["localhost", "LocalHost", "127.0.0.1", "127.8.9.10", "::1", "[::1]"] {
            assert!(is_loopback_host(h), "{h}");
        }
        for h in ["10.0.0.1", "proxy.corp", "localhost.corp", "::2", "0.0.0.0", ""] {
            assert!(!is_loopback_host(h), "{h}");
        }
    }

    #[test]
    fn windows_bypass_is_translated_conservatively() {
        assert_eq!(normalise_bypass(""), (vec![], 0));
        assert_eq!(normalise_bypass("<local>"), (vec![], 0));
        assert_eq!(
            normalise_bypass("*.corp; intranet;10.1.2.3;192.168.*;<local>;*;*foo*;Intranet;fe80::1"),
            (vec![".corp".into(), "intranet".into(), "10.1.2.3".into(), "fe80::1".into()], 3)
        );
        assert_eq!(normalise_bypass("host:8080;http://x"), (vec![], 2));
    }

    // ---- the step 3 table, one test per row ----

    #[test]
    fn off_is_none_whatever_windows_says() {
        let r = resolve(Mode::Off, Some("15236"), || panic!("Off must not look"), |_| panic!("no probe"));
        assert_eq!(r, Resolved::none(Source::Off));
    }

    #[test]
    fn manual_uses_only_the_port() {
        let r = resolve(Mode::Manual, Some(" 15236 "), || panic!("Manual must not look"), |_| panic!("no probe"));
        assert_eq!(r.source, Source::Manual);
        assert_eq!(r.url.as_deref(), Some("http://127.0.0.1:15236"));
        for bad in [None, Some(""), Some("0"), Some("65536"), Some("12a"), Some("+80")] {
            let r = resolve(Mode::Manual, bad, || panic!("Manual must not look"), |_| panic!("no probe"));
            assert_eq!(r.url, None, "{bad:?}");
            assert_eq!(r.notes.len(), 1, "{bad:?}");
        }
    }

    #[test]
    fn system_static_http_is_used_and_localhost_exported_as_ipv4() {
        let r = sys(stat("http://localhost:15236"));
        assert_eq!(r.source, Source::System);
        assert_eq!(r.url.as_deref(), Some("http://127.0.0.1:15236"));
        assert!(!r.socks_only);
        assert!(r.notes.is_empty(), "{:?}", r.notes);
        assert_eq!(r.diag.lookup.as_deref(), Some("static"));
        assert_eq!(r.diag.raw_proxy.as_deref(), Some("http://localhost:15236"));
        assert_eq!(r.diag.answered.as_deref(), Some("127.0.0.1"));
        assert!(r.diag.lookup_ms.is_some());
    }

    #[test]
    fn system_static_with_a_script_uses_the_static_and_says_so() {
        let r = sys(Lookup::Static {
            proxy: "localhost:15236".into(),
            bypass: None,
            script: Some("http://127.0.0.1:1/pac".into()),
        });
        assert_eq!(r.url.as_deref(), Some("http://127.0.0.1:15236"));
        assert!(r.notes.iter().any(|n| n.contains("setup script")));
        assert_eq!(r.diag.script_url.as_deref(), Some("http://127.0.0.1:1/pac"));
    }

    #[test]
    fn system_socks_only_is_flagged_never_http() {
        let r = sys(stat("socks=localhost:1080"));
        assert!(r.socks_only);
        assert_eq!(r.url.as_deref(), Some("socks5h://127.0.0.1:1080"));
        assert!(r.notes.iter().any(|n| n.contains("does not support SOCKS")));
    }

    #[test]
    fn system_script_only_autodetect_none_and_unavailable_give_none() {
        let r = sys(Lookup::ScriptOnly { url: "http://x/pac".into() });
        assert_eq!((r.url.as_deref(), r.diag.lookup.as_deref()), (None, Some("script")));
        assert!(r.notes[0].contains("script (PAC)"));

        let r = sys(Lookup::AutoDetectOnly);
        assert_eq!((r.url.as_deref(), r.diag.lookup.as_deref()), (None, Some("autodetect")));
        assert!(r.notes.is_empty(), "the Windows default must not warn: {:?}", r.notes);

        let r = sys(Lookup::None);
        assert_eq!((r.url.as_deref(), r.diag.lookup.as_deref()), (None, Some("none")));
        assert!(r.notes.is_empty());

        let r = sys(Lookup::Unavailable(12029));
        assert_eq!((r.url.as_deref(), r.diag.error_code), (None, Some(12029)));
        assert!(r.notes[0].contains("12029"));
    }

    #[test]
    fn system_unparseable_static_gives_none_with_a_note() {
        let r = sys(stat("garbage"));
        assert_eq!(r.url, None);
        assert!(r.notes[0].contains("Could not read"));
    }

    #[test]
    fn system_ignores_the_manual_port() {
        let r = resolve(Mode::System, Some("15236"), || Lookup::None, all_up);
        assert_eq!(r.url, None);
    }

    // ---- liveness ----

    #[test]
    fn liveness_tries_ipv4_then_ipv6_and_exports_whichever_answered() {
        let r = resolve(Mode::System, None, || stat("localhost:15236"), |a| a.is_ipv6());
        assert_eq!(r.url.as_deref(), Some("http://[::1]:15236"));
        assert_eq!(r.diag.answered.as_deref(), Some("::1"));

        let tried = std::cell::RefCell::new(Vec::new());
        let r = resolve(Mode::System, None, || stat("localhost:15236"), |a| {
            tried.borrow_mut().push(a);
            false
        });
        assert_eq!(r.url, None);
        assert!(r.notes.iter().any(|n| n.contains("not reachable")));
        assert_eq!(*tried.borrow(), vec!["127.0.0.1:15236".parse().unwrap(), "[::1]:15236".parse().unwrap()]);
    }

    #[test]
    fn liveness_is_skipped_for_remote_proxies() {
        let r = resolve(Mode::System, None, || stat("proxy.corp:8080"), |_| panic!("remote must not be probed"));
        assert_eq!(r.url.as_deref(), Some("http://proxy.corp:8080"));
        assert_eq!(r.diag.answered, None);
    }

    #[test]
    fn tcp_probe_sees_a_listener_and_its_absence() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        assert!(tcp_probe(addr));
        drop(l);
        assert!(!tcp_probe(addr));
    }

    // ---- environment ----

    #[test]
    fn env_values_are_full_urls_with_lowercase_keys_and_no_localhost() {
        for proxy in ["localhost:15236", "http://localhost:15236", "LOCALHOST:15236", "http=localhost:1;https=localhost:2"] {
            let env = proxy_env(&sys(stat(proxy)), Some(""));
            assert!(!env.is_empty(), "{proxy}");
            for (k, v) in &env {
                if k.ends_with("_proxy") && k != "no_proxy" {
                    assert!(v.starts_with("http://"), "{proxy}: {k}={v}");
                    assert_eq!(k, &k.to_ascii_lowercase());
                    assert!(!v.to_ascii_lowercase().contains("localhost"), "{proxy}: {k}={v}");
                }
            }
        }
    }

    #[test]
    fn env_for_http_has_both_proxies_default_bypass_and_node_flag() {
        let env = proxy_env(&sys(stat("localhost:15236")), None);
        let get = |k: &str| env.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());
        assert_eq!(get("http_proxy"), Some("http://127.0.0.1:15236"));
        assert_eq!(get("https_proxy"), Some("http://127.0.0.1:15236"));
        assert_eq!(get("no_proxy"), Some(DEFAULT_BYPASS));
        assert_eq!(get("NODE_USE_ENV_PROXY"), Some("1"));
        assert_eq!(get("all_proxy"), None);
    }

    #[test]
    fn env_bypass_is_custom_plus_windows_deduplicated() {
        let r = sys(Lookup::Static {
            proxy: "localhost:15236".into(),
            bypass: Some("*.corp;<local>;192.168.*;Build.Local".into()),
            script: None,
        });
        assert!(r.notes.iter().any(|n| n.starts_with("1 of Windows' bypass")));
        let env = proxy_env(&r, Some("localhost, build.local  .internal"));
        let no_proxy = env.iter().find(|(k, _)| k == "no_proxy").unwrap();
        assert_eq!(no_proxy.1, "localhost,build.local,.internal,.corp");
        let env = proxy_env(&sys(stat("localhost:15236")), Some(""));
        assert!(env.iter().all(|(k, _)| k != "no_proxy"));
    }

    #[test]
    fn env_for_socks_only_sets_all_proxy_and_no_http() {
        let env = proxy_env(&sys(stat("socks=localhost:1080")), None);
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["all_proxy", "no_proxy"]);
        assert_eq!(env[0].1, "socks5h://127.0.0.1:1080");
    }

    #[test]
    fn env_is_empty_without_a_url() {
        assert!(proxy_env(&sys(Lookup::None), None).is_empty());
        assert!(proxy_env(&resolve(Mode::Off, None, || Lookup::None, all_up), None).is_empty());
    }
}
