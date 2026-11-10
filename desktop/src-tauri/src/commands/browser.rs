//! Opening a web address in the player's default browser.

/// Whether `url` is one the app may open: a secure web address, or plain http
/// on this machine (development). Nothing else (`file:`, `javascript:`, other
/// schemes, a program path) is ever handed to the shell.
pub fn is_openable(url: &str) -> bool {
    let starts = |prefix: &str| url.len() > prefix.len() && url[..prefix.len()].eq_ignore_ascii_case(prefix);
    // `http://127.0.0.1.evil.example` must not pass for the local machine.
    let local = |host: &str| {
        starts(&format!("http://{host}"))
            && matches!(url.as_bytes().get(7 + host.len()), None | Some(b'/' | b':' | b'?'))
    };
    (starts("https://") || local("127.0.0.1") || local("localhost"))
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// Opens `url` with the system's default handler for web addresses.
///
/// On Windows this is `ShellExecuteW` with the `open` verb, which is what the
/// system itself uses for links. (Handing the address to `explorer.exe` is not
/// equivalent: with a query string, such as the sign-in link has, Explorer does
/// not take it as a web address and opens a folder instead.)
pub fn open_url(url: &str) -> Result<(), String> {
    if !is_openable(url) {
        return Err("invalid_url".into());
    }
    #[cfg(windows)]
    {
        use windows::core::{w, PCWSTR};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: both strings are NUL-terminated and outlive the call.
        let result = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(wide.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        // Values above 32 mean success; below, an error code.
        let code = result.0 as isize;
        if code <= 32 {
            return Err(format!("open_failed: ShellExecute returned {code}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_addresses_are_openable() {
        assert!(is_openable(
            "https://warlay.codesoftlabs.com/auth/google/start?port=51234&state=abc&challenge=x_y-z"
        ));
        assert!(is_openable("HTTPS://github.com/jhonsferg/warlay/releases"));
        assert!(is_openable("http://127.0.0.1:1420/"));
        assert!(is_openable("http://localhost:8080/auth"));
        assert!(!is_openable("http://evil.example/"));
        assert!(!is_openable("http://127.0.0.1.evil.example/"));
        assert!(!is_openable("http://localhost.evil.example/"));
        assert!(!is_openable("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_openable("javascript:alert(1)"));
        assert!(!is_openable("C:\\Windows\\System32\\calc.exe"));
        assert!(!is_openable("calc.exe"));
        assert!(!is_openable("https://"));
        assert!(!is_openable("https://example.com/ with space"));
        assert!(!is_openable("https://example.com/\r\nx"));
        assert_eq!(open_url("file:///C:/x"), Err("invalid_url".to_string()));
    }
}
