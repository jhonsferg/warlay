use tracing::warn;
use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, BOOL, HANDLE};
use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::System::Pipes::GetNamedPipeClientProcessId;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Restricts the named pipe to this machine's interactive users, `SYSTEM`,
/// and built-in Administrators -- by default `ServerOptions::create` gives
/// the pipe no explicit ACL at all, which (combined with the daemon running
/// elevated) means any local process could command it, including the
/// privileged `LaunchGame`. Denies remote/anonymous access by omission.
///
/// Leaked deliberately: `CreateNamedPipeW` doesn't retain the pointer past
/// the call, but every pipe instance (one per accepted connection, for the
/// life of the process) needs the same descriptor, so it's simplest to
/// build it once and never free it -- a single tiny allocation that lives
/// exactly as long as the process does. Returned as a `usize` rather than a
/// raw pointer so it can cross `.await` points in `main`'s state machine
/// (multi-threaded tokio requires that state machine to be `Send`, which a
/// bare `*mut c_void` field is not).
pub fn build_pipe_security_attributes() -> usize {
    const PIPE_SDDL: &str = "D:(A;;GA;;;IU)(A;;GA;;;SY)(A;;GA;;;BA)";
    let sddl = HSTRING::from(PIPE_SDDL);
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(&sddl, SDDL_REVISION_1, &mut descriptor, None)
            .expect("PIPE_SDDL is a fixed, valid SDDL string");
    }
    let sa = Box::new(SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: BOOL(0),
    });
    Box::into_raw(sa) as usize
}

/// The pipe ACL above only restricts by Windows *principal* (interactive
/// user / SYSTEM / built-in Administrators) -- it says nothing about which
/// *process* connects. Any other process already running as the same
/// logged-in user, including unprivileged malware with no elevation of its
/// own, can still open this pipe and drive it, and the daemon runs
/// elevated and exposes privileged actions (`LaunchGame`, and any future
/// firewall/network configuration work) on the other end.
///
/// This closes that gap: after a client connects, ask the kernel (not the
/// client -- nothing here trusts anything the other end says about itself)
/// which process actually holds the far end of the pipe via
/// `GetNamedPipeClientProcessId`, then verify its full image path is
/// exactly this installation's `warlay.exe`, sitting next to this very
/// `wnetd.exe` binary (the installer always places both in the same
/// directory). Doesn't stop every conceivable attack -- code injected into
/// an already-running, legitimate `warlay.exe` would still pass -- but it
/// closes the trivial case of "any other process running as this user
/// talks to our pipe and asks us to do something privileged on its
/// behalf," which today it can.
pub fn verify_client_process(pipe: &tokio::net::windows::named_pipe::NamedPipeServer) -> bool {
    use std::os::windows::io::AsRawHandle;

    let handle = HANDLE(pipe.as_raw_handle());

    let mut pid: u32 = 0;
    if unsafe { GetNamedPipeClientProcessId(handle, &mut pid) }.is_err() {
        warn!("could not resolve client pid for pipe connection; rejecting");
        return false;
    }

    let process = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        Err(e) => {
            warn!(pid, error = %e, "could not open client process to verify its identity; rejecting");
            return false;
        }
    };

    let mut buf = [0u16; 512];
    let mut size = buf.len() as u32;
    let image_result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
    };
    unsafe {
        let _ = CloseHandle(process);
    }

    if image_result.is_err() {
        warn!(pid, "could not resolve client process image path; rejecting");
        return false;
    }

    let client_path = std::path::PathBuf::from(String::from_utf16_lossy(&buf[..size as usize]));

    let expected_path = std::env::current_exe()
        .ok()
        .map(|p| p.with_file_name("warlay.exe"))
        .and_then(|p| p.canonicalize().ok());
    let client_canonical = client_path.canonicalize().ok();

    match (&expected_path, &client_canonical) {
        (Some(expected), Some(client)) if expected == client => true,
        _ => {
            warn!(
                pid,
                client = %client_path.display(),
                expected = ?expected_path,
                "rejected pipe connection from unrecognized process"
            );
            false
        }
    }
}
