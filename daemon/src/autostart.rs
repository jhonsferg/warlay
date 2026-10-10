use tracing::{error, info, warn};

const AUTOSTART_TASK_NAME: &str = "Warlay-Daemon";

/// `CREATE_NO_WINDOW` -- without it, spawning a console-subsystem child
/// (`schtasks.exe`) from this app makes Windows allocate a console for it,
/// flashing visibly on screen for the instant it runs even though nothing
/// ever needed to show in it.
fn silent_command(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// Registers a Scheduled Task that starts this exact binary at logon with
/// the user's highest available privileges. Does not launch it immediately:
/// the desktop app's own startup loading screen is what starts the daemon
/// (with a real, visible UAC prompt if elevation is needed) the first time
/// it's actually needed -- the installer eagerly launching one of its own,
/// non-interactively, would race with that and give no way to surface an
/// elevation failure to the user.
///
/// A Scheduled Task rather than a true SCM Windows Service deliberately:
/// a service would run as `LocalSystem` before any user logs in, and
/// launching WC3 from it would need impersonating the interactive user's
/// session (`WTSQueryUserToken` + `CreateProcessAsUser`) for no real
/// benefit here. `schtasks /rl highest` starts automatically at logon,
/// already runs as the right user for launching their own game, and needs
/// no manual elevation once installed (Task Scheduler is a trusted
/// elevation path) -- SIO_RCVALL still needs that user's account to be an
/// administrator either way; this only removes the manual "run it myself"
/// step, it doesn't change that underlying platform constraint.
pub fn install_autostart() -> std::io::Result<()> {
    let exe_path = std::env::current_exe()?;
    let exe_path_str = exe_path.to_string_lossy().into_owned();

    info!(exe = %exe_path_str, "registering autostart scheduled task");
    let status = silent_command("schtasks")
        .args([
            "/create",
            "/tn",
            AUTOSTART_TASK_NAME,
            "/tr",
            &format!("\"{exe_path_str}\""),
            "/sc",
            "onlogon",
            "/rl",
            "highest",
            "/f",
        ])
        .status()?;
    if !status.success() {
        error!(?status, "schtasks /create failed");
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "schtasks /create failed",
        ));
    }

    info!("scheduled task registered");
    Ok(())
}

/// Removes the scheduled task registered by `install_autostart`. Does not
/// stop an already-running daemon process -- that's the installer's job
/// (it knows whether it's mid-upgrade vs. a real uninstall), not this
/// binary's.
pub fn uninstall_autostart() -> std::io::Result<()> {
    info!("removing autostart scheduled task");
    let status = silent_command("schtasks")
        .args(["/delete", "/tn", AUTOSTART_TASK_NAME, "/f"])
        .status()?;
    if !status.success() {
        warn!(
            ?status,
            "schtasks /delete failed (the task may simply not have existed)"
        );
    }
    Ok(())
}
