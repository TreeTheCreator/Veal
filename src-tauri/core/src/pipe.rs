//! The daemon's named pipe: its name, who may open it, and who may serve it
//! (plan-19 section 6). Anything written to this pipe runs as a shell, so the
//! server admits only this user, and a client refuses a server run by anyone
//! else before sending a byte.

use std::io;

/// `VEAL_PIPE` if set (tests), else `\\.\pipe\veal-<SID>`, or `veal-dev-<SID>`
/// in a debug build, so a dev window never attaches to the installed daemon.
pub fn pipe_name() -> io::Result<String> {
    if let Some(name) = std::env::var_os("VEAL_PIPE") {
        return Ok(name.to_string_lossy().into_owned());
    }
    let prefix = if cfg!(debug_assertions) { "veal-dev" } else { "veal" };
    Ok(format!(r"\\.\pipe\{prefix}-{}", user_sid()?))
}

#[cfg(windows)]
pub use imp::{check_server_owner, server, user_sid};

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::io::AsRawHandle;
    use tokio::net::windows::named_pipe::{NamedPipeClient, NamedPipeServer, ServerOptions};
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, ERROR_ACCESS_DENIED, HANDLE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        EqualSid, GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    /// The current user's SID, like `S-1-5-21-…`.
    pub fn user_sid() -> io::Result<String> {
        let user = owned_user(unsafe { GetCurrentProcess() })?;
        unsafe {
            let tu = &*(user.as_ptr().cast::<TOKEN_USER>());
            let mut s: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(tu.User.Sid, &mut s) == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut n = 0;
            while *s.add(n) != 0 {
                n += 1;
            }
            let out = String::from_utf16_lossy(std::slice::from_raw_parts(s, n));
            LocalFree(s.cast());
            Ok(out)
        }
    }

    /// Whether two processes run as the same user.
    fn same_user(other: HANDLE) -> io::Result<bool> {
        unsafe {
            let mine = owned_user(GetCurrentProcess())?;
            let theirs = owned_user(other)?;
            let a = &*(mine.as_ptr().cast::<TOKEN_USER>());
            let b = &*(theirs.as_ptr().cast::<TOKEN_USER>());
            Ok(EqualSid(a.User.Sid, b.User.Sid) != 0)
        }
    }

    /// `TOKEN_USER` in aligned storage that outlives the SID pointer in it.
    fn owned_user(process: HANDLE) -> io::Result<Vec<u64>> {
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
                return Err(io::Error::last_os_error());
            }
            let token = Handle(token);
            let mut len = 0u32;
            GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut len);
            if len == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            if GetTokenInformation(token.0, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(buf)
        }
    }

    /// A pipe instance only this user can open, local clients only. With
    /// `first`, creating it fails with `AddrInUse` if another server already
    /// has the name: that is how a second daemon learns it lost.
    pub fn server(name: &str, first: bool) -> io::Result<NamedPipeServer> {
        let sddl: Vec<u16> = format!("D:P(A;;GA;;;{})\0", user_sid()?).encode_utf16().collect();
        unsafe {
            let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut()) == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd,
                bInheritHandle: 0,
            };
            let made = ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(name, (&mut sa as *mut SECURITY_ATTRIBUTES).cast());
            LocalFree(sd);
            made.map_err(|e| {
                if first && e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
                    io::Error::new(io::ErrorKind::AddrInUse, "another daemon has the pipe")
                } else {
                    e
                }
            })
        }
    }

    /// Refuse a pipe served by another user's process, before sending a byte.
    pub fn check_server_owner(pipe: &NamedPipeClient) -> io::Result<()> {
        unsafe {
            let mut pid = 0u32;
            if GetNamedPipeServerProcessId(pipe.as_raw_handle() as HANDLE, &mut pid) == 0 {
                return Err(io::Error::last_os_error());
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return Err(io::Error::last_os_error());
            }
            let process = Handle(process);
            if same_user(process.0)? {
                Ok(())
            } else {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("pipe server {pid} runs as another user")))
            }
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use tokio::net::windows::named_pipe::ClientOptions;

    fn unique() -> String {
        format!(r"\\.\pipe\veal-test-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
    }

    #[test]
    fn sid_looks_like_a_user() {
        let sid = user_sid().unwrap();
        assert!(sid.starts_with("S-1-5-"), "{sid}");
    }

    #[test]
    fn default_name_is_a_pipe() {
        // Read-only check of the format; the override itself is exercised by
        // the daemon's integration tests, which set it per process.
        let name = pipe_name().unwrap();
        assert!(name.starts_with(r"\\.\pipe\"), "{name}");
    }

    #[tokio::test]
    async fn second_first_instance_loses_and_owner_check_passes() {
        let name = unique();
        let _first = server(&name, true).unwrap();
        let second = server(&name, true).unwrap_err();
        assert_eq!(second.kind(), io::ErrorKind::AddrInUse);
        let client = ClientOptions::new().open(&name).unwrap();
        check_server_owner(&client).unwrap();
    }
}
