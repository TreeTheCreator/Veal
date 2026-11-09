//! The user's environment as Windows would give a newly logged-on process
//! (plan-19 section 2). A daemon lives for days; inheriting its own
//! environment would hand new panes the `PATH` from when it started, before
//! the user installed whatever they installed since.

use std::ffi::OsString;

/// The current user's environment, read fresh from the registry. `None` if
/// Windows would not build it, and always off Windows.
pub fn fresh() -> Option<Vec<(OsString, OsString)>> {
    imp::fresh()
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{TOKEN_DUPLICATE, TOKEN_IMPERSONATE, TOKEN_QUERY};
    use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    pub fn fresh() -> Option<Vec<(OsString, OsString)>> {
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_IMPERSONATE, &mut token) == 0 {
                return None;
            }
            let mut block: *mut core::ffi::c_void = std::ptr::null_mut();
            let ok = CreateEnvironmentBlock(&mut block, token, 0);
            CloseHandle(token);
            if ok == 0 || block.is_null() {
                return None;
            }
            let vars = parse(block as *const u16);
            DestroyEnvironmentBlock(block);
            Some(vars)
        }
    }

    /// `NAME=value\0NAME=value\0\0`, in UTF-16.
    unsafe fn parse(mut p: *const u16) -> Vec<(OsString, OsString)> {
        let mut vars = Vec::new();
        loop {
            let mut len = 0;
            while *p.add(len) != 0 {
                len += 1;
            }
            if len == 0 {
                break;
            }
            let entry = std::slice::from_raw_parts(p, len);
            // Skip position 0: per-drive cwd entries look like `=C:=C:\`.
            if let Some(eq) = entry.iter().skip(1).position(|&c| c == b'=' as u16) {
                let eq = eq + 1;
                vars.push((OsString::from_wide(&entry[..eq]), OsString::from_wide(&entry[eq + 1..])));
            }
            p = p.add(len + 1);
        }
        vars
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn fresh() -> Option<Vec<(std::ffi::OsString, std::ffi::OsString)>> {
        None
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn fresh_has_the_profile_and_not_this_process() {
        std::env::set_var("VEAL_USERENV_TEST_ONLY_HERE", "1");
        let vars = fresh().expect("CreateEnvironmentBlock");
        let has = |k: &str| vars.iter().any(|(n, _)| n.to_string_lossy().eq_ignore_ascii_case(k));
        assert!(has("PATH"), "{vars:?}");
        assert!(has("USERPROFILE"));
        assert!(!has("VEAL_USERENV_TEST_ONLY_HERE"));
    }
}
