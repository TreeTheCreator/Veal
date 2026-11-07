// Windows' proxy setting for the current user (plan-18): one WinHTTP call,
// copied and freed here. Every decision about it lives in proxy.rs.
use crate::proxy::Lookup;

/// The active connection's settings (LAN, dial-up or VPN). A local
/// configuration read: no WPAD discovery, no script download. With "Use a
/// proxy server" off, `lpszProxy` comes back NULL even while the registry
/// still holds the old address (checked on this machine, plan-18 A2b).
#[cfg(windows)]
pub fn lookup() -> Lookup {
    use windows_sys::Win32::Foundation::{GetLastError, GlobalFree, ERROR_FILE_NOT_FOUND};
    use windows_sys::Win32::Networking::WinHttp::{
        WinHttpGetIEProxyConfigForCurrentUser, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG,
    };

    /// Copies one string the call allocated, then frees it. Blank is `None`.
    unsafe fn take(p: *mut u16) -> Option<String> {
        if p.is_null() {
            return None;
        }
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
        GlobalFree(p as _);
        Some(s).filter(|s| !s.trim().is_empty())
    }

    let mut cfg = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
    if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut cfg) } == 0 {
        let code = unsafe { GetLastError() };
        // No settings at all is not an error.
        return if code == ERROR_FILE_NOT_FOUND { Lookup::None } else { Lookup::Unavailable(code) };
    }
    // All three are freed before any is looked at.
    let (script, proxy, bypass) =
        unsafe { (take(cfg.lpszAutoConfigUrl), take(cfg.lpszProxy), take(cfg.lpszProxyBypass)) };
    match (proxy, script) {
        (Some(proxy), script) => Lookup::Static { proxy, bypass, script },
        (None, Some(url)) => Lookup::ScriptOnly { url },
        (None, None) if cfg.fAutoDetect != 0 => Lookup::AutoDetectOnly,
        (None, None) => Lookup::None,
    }
}

/// Nothing to read off Windows; `pnpm tauri dev` and the unix tests still build.
#[cfg(not(windows))]
pub fn lookup() -> Lookup {
    Lookup::Unavailable(0)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Machine-dependent by nature: only that the call works for a user
    /// session and, when a proxy is set, hands back something parseable.
    #[test]
    fn the_call_works_for_this_user() {
        match lookup() {
            Lookup::Unavailable(code) => panic!("WinHttpGetIEProxyConfigForCurrentUser failed: {code}"),
            Lookup::Static { proxy, .. } => {
                crate::proxy::parse_static(&proxy).unwrap_or_else(|e| panic!("{e}"));
            }
            _ => {}
        }
    }

    /// `cargo test what_a_pane_gets -- --ignored --nocapture`: the real
    /// lookup, liveness and env on this machine.
    #[test]
    #[ignore]
    fn what_a_pane_gets() {
        use crate::proxy::{proxy_env, resolve, tcp_probe, Mode};
        let r = resolve(Mode::System, None, lookup, tcp_probe);
        println!("{}", serde_json::to_string_pretty(&r).unwrap());
        for (k, v) in proxy_env(&r, None) {
            println!("{k}={v}");
        }
    }
}
