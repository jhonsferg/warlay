//! Privilege state of the current process.
//!
//! `SIO_RCVALL` raw capture is what the agent needs Administrator for, and
//! the binaries embedding this crate carry a `requireAdministrator` manifest,
//! so Windows should refuse to start them unelevated in the first place.
//! This is the belt to that braces: a daemon that starts, answers pings and
//! only fails on the first game is the worst possible failure mode, so the
//! agent refuses to open its pipe at all when the token says otherwise.

/// `true` when this process' token is elevated (`TokenElevation`), i.e. when
/// we really are running as administrator right now.
///
/// Fail-closed: any error talking to the token is reported as "not
/// elevated", which makes the agent exit rather than serve a daemon that
/// cannot capture.
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE(std::ptr::null_mut());
        if let Err(e) = OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) {
            tracing::warn!(error = %e, "no se pudo abrir el token del proceso; asumiendo sin elevar");
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        let queried = GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        let _ = CloseHandle(token);

        match queried {
            Ok(()) => elevation.TokenIsElevated != 0,
            Err(e) => {
                tracing::warn!(error = %e, "no se pudo consultar TokenElevation; asumiendo sin elevar");
                false
            }
        }
    }
}
