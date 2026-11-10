//! Crash reports, kept on the player's machine until the player decides.
//!
//! When the app panics (or the interface hits an uncaught error) a small text
//! report is written to `%APPDATA%\com.warlay.lanlauncher\crashes\`: the version,
//! the system, the message and where it happened. Nothing is sent anywhere. The
//! next time the app opens, the player is shown the report and can send it as a
//! prefilled GitHub issue (opened in the browser, to review before submitting),
//! copy it, or discard it. Names of the Windows user and of their profile folder
//! are removed from the text before it is written.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde::Serialize;

/// Reports kept; older ones are deleted at startup.
const KEEP: usize = 20;
/// A report is a note, not a memory dump.
const MAX_REPORT_CHARS: usize = 24_000;
/// How much of a report goes into the issue's address (the browser, GitHub and
/// proxies cut addresses that are too long; the full text can be copied).
const ISSUE_BODY_CHARS: usize = 2_500;
/// Interface errors recorded per run: a render loop must not fill the disk.
const MAX_UI_ERRORS_PER_RUN: u32 = 20;

static UI_ERRORS: AtomicU32 = AtomicU32::new(0);

/// `%APPDATA%\com.warlay.lanlauncher\crashes`, found without an app handle (a
/// panic can happen before one exists).
pub fn dir() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    Path::new(&appdata).join("com.warlay.lanlauncher").join("crashes")
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CrashSummary {
    pub id: String,
    /// Unix milliseconds.
    pub created_at: u64,
    /// `panic` or `ui`.
    pub kind: String,
    /// The first line of the message, for a list.
    pub title: String,
}

/// Replaces the profile folder and the user name, which may appear in paths and
/// messages, so the report does not identify the person.
pub fn sanitize(text: &str, home: Option<&str>, user: Option<&str>) -> String {
    let mut out = text.to_string();
    if let Some(home) = home.filter(|h| h.len() > 3) {
        for variant in [home.to_string(), home.replace('\\', "/"), home.replace('\\', "\\\\")] {
            out = out.replace(&variant, "%USERPROFILE%");
        }
    }
    if let Some(user) = user.filter(|u| u.chars().count() >= 3) {
        out = out.replace(user, "<user>");
    }
    out
}

fn sanitize_env(text: &str) -> String {
    sanitize(
        text,
        std::env::var("USERPROFILE").ok().as_deref(),
        std::env::var("USERNAME").ok().as_deref(),
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Whether `id` is a report file name this module made (never a path).
pub fn is_valid_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("crash-").and_then(|r| r.strip_suffix(".txt")) else {
        return false;
    };
    let Some((millis, kind)) = rest.split_once('-') else {
        return false;
    };
    !millis.is_empty() && millis.bytes().all(|b| b.is_ascii_digit()) && matches!(kind, "panic" | "ui")
}

/// Writes a report and returns its id. `details` is sanitized and cut.
pub fn write_report(dir: &Path, kind: &str, title: &str, details: &str, version: &str) -> io::Result<String> {
    std::fs::create_dir_all(dir)?;
    let mut counter = 0;
    let id = loop {
        let candidate = format!("crash-{}-{kind}.txt", now_ms() + counter);
        if !dir.join(&candidate).exists() {
            break candidate;
        }
        counter += 1;
    };
    let body: String = sanitize_env(details).chars().take(MAX_REPORT_CHARS).collect();
    let title = sanitize_env(title);
    let text = format!(
        "Warlay {version}\n{} {}\n{kind}\n\n{}\n\n{body}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        title.lines().next().unwrap_or("(sin mensaje)")
    );
    std::fs::write(dir.join(&id), text)?;
    Ok(id)
}

/// The panic that is going down, as a report.
pub fn write_panic(info: &std::panic::PanicHookInfo<'_>, version: &str) {
    let message = info.to_string();
    let backtrace = std::backtrace::Backtrace::force_capture();
    let thread = std::thread::current();
    let details = format!(
        "{message}\n\nthread: {}\n\nbacktrace:\n{backtrace}",
        thread.name().unwrap_or("<unnamed>")
    );
    let _ = write_report(&dir(), "panic", &message, &details, version);
}

/// An uncaught error of the interface. Returns whether it was recorded.
pub fn write_ui_error(message: &str, stack: &str, version: &str) -> bool {
    if UI_ERRORS.fetch_add(1, Ordering::Relaxed) >= MAX_UI_ERRORS_PER_RUN {
        return false;
    }
    let details = format!("{message}\n\nstack:\n{stack}");
    write_report(&dir(), "ui", message, &details, version).is_ok()
}

fn created_at(id: &str) -> u64 {
    id.strip_prefix("crash-")
        .and_then(|r| r.split('-').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// The reports waiting, newest first.
pub fn list(dir: &Path) -> Vec<CrashSummary> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut reports: Vec<CrashSummary> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().into_owned();
            if !is_valid_id(&id) {
                return None;
            }
            let text = std::fs::read_to_string(entry.path()).ok()?;
            let mut lines = text.lines();
            let kind = lines.nth(2).unwrap_or("panic").to_string();
            // Header (3 lines), blank, then the message.
            let title = lines.nth(1).unwrap_or("").to_string();
            Some(CrashSummary {
                created_at: created_at(&id),
                id,
                kind,
                title,
            })
        })
        .collect();
    reports.sort_by_key(|r| std::cmp::Reverse(r.created_at));
    reports
}

pub fn read(dir: &Path, id: &str) -> Option<String> {
    if !is_valid_id(id) {
        return None;
    }
    std::fs::read_to_string(dir.join(id)).ok()
}

pub fn delete(dir: &Path, id: &str) -> bool {
    is_valid_id(id) && std::fs::remove_file(dir.join(id)).is_ok()
}

/// Deletes all but the newest `keep` reports.
pub fn sweep(dir: &Path, keep: usize) {
    for old in list(dir).into_iter().skip(keep) {
        let _ = delete(dir, &old.id);
    }
}

pub fn sweep_default() {
    sweep(&dir(), KEEP);
}

/// A new-issue address on this project's repository with the report filled in.
pub fn issue_url(report: &str, version: &str) -> String {
    let first = report.lines().nth(4).unwrap_or("").trim();
    let title = format!("Crash: {}", first.chars().take(100).collect::<String>());
    let excerpt: String = report.chars().take(ISSUE_BODY_CHARS).collect();
    let cut = if report.chars().count() > ISSUE_BODY_CHARS {
        "\n... (cortado; el informe completo está en la carpeta de informes de Warlay)\n"
    } else {
        ""
    };
    let body = format!(
        "**Warlay {version}**\n\n<!-- Revisa el texto antes de enviarlo: no incluye tu nombre de usuario de Windows ni tus cuentas, pero lo ves aquí completo. -->\n\n```text\n{excerpt}{cut}```\n\n**Qué estabas haciendo cuando pasó**\n\n"
    );
    let mut url = reqwest::Url::parse("https://github.com/jhonsferg/warlay/issues/new").expect("valid url");
    url.query_pairs_mut()
        .append_pair("title", &title)
        .append_pair("body", &body)
        .append_pair("labels", "bug");
    url.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("warlay-crash-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_user_and_the_profile_folder_are_removed() {
        let text = r"panicked at C:\Users\Arthas\AppData\x.rs:3: bad; also C:/Users/Arthas/y and Arthas said";
        let clean = sanitize(text, Some(r"C:\Users\Arthas"), Some("Arthas"));
        assert!(!clean.contains("Arthas"), "{clean}");
        assert!(clean.contains("%USERPROFILE%"));
        // Too short to be worth replacing (it would mangle ordinary words).
        assert_eq!(sanitize("a to b", None, Some("to")), "a to b");
        assert_eq!(sanitize("nothing", None, None), "nothing");
    }

    #[test]
    fn only_names_made_here_are_valid_ids() {
        assert!(is_valid_id("crash-1700000000000-panic.txt"));
        assert!(is_valid_id("crash-1700000000000-ui.txt"));
        for bad in [
            "",
            "crash-1-other.txt",
            "../crash-1-panic.txt",
            "crash-1-panic.txt.exe",
            "crash-x-panic.txt",
            "crash--panic.txt",
            r"C:\Windows\win.ini",
            "crash-1/../../x-panic.txt",
        ] {
            assert!(!is_valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn a_report_is_written_listed_read_and_deleted() {
        let dir = temp();
        let first = write_report(
            &dir,
            "panic",
            "boom at x.rs:1",
            "boom at x.rs:1\n\nbacktrace:\n  0: f",
            "0.1.2",
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = write_report(
            &dir,
            "ui",
            "TypeError: x is undefined",
            "TypeError: x is undefined\n\nstack:\n a",
            "0.1.2",
        )
        .unwrap();
        // Something else in the folder is not a report.
        std::fs::write(dir.join("notes.txt"), "x").unwrap();

        let reports = list(&dir);
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].id, second, "newest first");
        assert_eq!(
            (reports[0].kind.as_str(), reports[0].title.as_str()),
            ("ui", "TypeError: x is undefined")
        );
        assert_eq!(reports[1].kind, "panic");
        let text = read(&dir, &first).unwrap();
        assert!(text.starts_with("Warlay 0.1.2\n") && text.contains("backtrace:"));
        assert_eq!(read(&dir, "notes.txt"), None);
        assert_eq!(read(&dir, "../x"), None);

        assert!(delete(&dir, &first));
        assert!(!delete(&dir, &first), "already gone");
        assert!(!delete(&dir, "notes.txt"));
        assert_eq!(list(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_newest_reports_are_kept() {
        let dir = temp();
        for i in 0..6 {
            std::fs::write(
                dir.join(format!("crash-{}-panic.txt", 1_000 + i)),
                "Warlay 1\nwindows x86_64\npanic\n\nm\n",
            )
            .unwrap();
        }
        sweep(&dir, 4);
        let kept: Vec<u64> = list(&dir).iter().map(|r| r.created_at).collect();
        assert_eq!(kept, vec![1005, 1004, 1003, 1002]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_huge_report_is_cut() {
        let dir = temp();
        let id = write_report(&dir, "panic", "big", &"x".repeat(MAX_REPORT_CHARS * 2), "1").unwrap();
        assert!(read(&dir, &id).unwrap().chars().count() < MAX_REPORT_CHARS + 200);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_issue_address_points_at_this_repository_and_stays_short() {
        let report = format!(
            "Warlay 0.1.2\nwindows x86_64\npanic\n\nboom at main.rs:1\n\n{}",
            "frame\n".repeat(2_000)
        );
        let url = issue_url(&report, "0.1.2");
        assert!(url.starts_with("https://github.com/jhonsferg/warlay/issues/new?"));
        assert!(url.contains("title=Crash%3A+boom+at+main.rs%3A1"), "{url}");
        assert!(url.contains("labels=bug"));
        assert!(url.len() < 8_000, "{}", url.len());
        // What the browser opener accepts.
        assert!(crate::commands::browser::is_openable(&url));
    }
}
