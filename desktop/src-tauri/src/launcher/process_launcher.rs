use super::LauncherError;
use std::path::Path;
use tokio::process::{Child, Command};

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
#[cfg(windows)]
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, JOBOBJECT_BASIC_PROCESS_ID_LIST,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

/// A Windows Job Object holding the launched game's entire process tree --
/// see `create_kill_on_close_job`'s doc comment for why this exists and how
/// its lifetime is managed. `HANDLE` wraps a raw pointer so it isn't
/// `Send`/`Sync` by default; a Win32 handle *value* has no thread affinity
/// (querying or closing it from any thread is safe per the Win32 docs), so
/// asserting both here is sound.
#[cfg(windows)]
#[derive(Clone, Copy)]
pub struct GameJob(HANDLE);
#[cfg(windows)]
unsafe impl Send for GameJob {}
#[cfg(windows)]
unsafe impl Sync for GameJob {}

#[cfg(not(windows))]
#[derive(Clone, Copy)]
pub struct GameJob;

/// Creates a Windows Job Object, assigns `child` to it with
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` set, and returns the job handle.
/// Windows automatically pulls every process `child` goes on to spawn into
/// the same job -- so this sidesteps ever needing to know what the real,
/// persistent game process is named. Many real installs (see
/// `is_process_running_at`'s doc comment) run the launched `.exe` only as a
/// stub that spawns the actual game under a completely different name
/// (e.g. a configured "Frozen Throne.exe" handing off to a persistent
/// "war3.exe") and exits -- with a job object, "is anything from this
/// launch still alive" and "kill everything from this launch" both become
/// one job-wide operation instead of a name-matching guess that breaks the
/// moment the real binary's name doesn't match what's configured.
///
/// The returned handle is deliberately never closed here except when a
/// later launch explicitly replaces it (see `commands::launcher::launch_game`).
/// Left open otherwise, it -- and the `KILL_ON_JOB_CLOSE` limit riding on it
/// -- gets closed by Windows itself as part of *this* process's own
/// handle-table teardown on exit, however that exit happens (normal quit,
/// panic-abort, crash, or an external `taskkill`). That's what ties the
/// game's lifetime to Warlay's without needing any of our own shutdown-path
/// code to run at all.
///
/// Failure at any step (job creation, setting the limit, or assignment) is
/// logged and treated as "no job" -- the actual game launch this wraps must
/// not fail just because this best-effort tracking/lifecycle tie couldn't
/// be set up.
#[cfg(windows)]
fn create_kill_on_close_job(child: &Child) -> Option<GameJob> {
    let Some(raw_handle) = child.raw_handle() else {
        tracing::warn!("create_kill_on_close_job: child already exited before it could be tracked");
        return None;
    };
    let process_handle = HANDLE(raw_handle);

    let job = match unsafe { CreateJobObjectW(None, windows::core::PCWSTR(std::ptr::null())) } {
        Ok(job) => job,
        Err(error) => {
            tracing::warn!(%error, "create_kill_on_close_job: CreateJobObjectW failed");
            return None;
        }
    };

    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let set_result = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if let Err(error) = set_result {
        tracing::warn!(%error, "create_kill_on_close_job: SetInformationJobObject failed");
        unsafe {
            let _ = CloseHandle(job);
        }
        return None;
    }

    if let Err(error) = unsafe { AssignProcessToJobObject(job, process_handle) } {
        tracing::warn!(%error, "create_kill_on_close_job: AssignProcessToJobObject failed");
        unsafe {
            let _ = CloseHandle(job);
        }
        return None;
    }

    Some(GameJob(job))
}

#[cfg(not(windows))]
fn create_kill_on_close_job(_child: &Child) -> Option<GameJob> {
    None
}

/// Every process ID currently alive inside `job` -- i.e. the launch's real,
/// current process tree, regardless of how many stub hand-offs it went
/// through or what any of those processes are named. An empty result means
/// the whole launch has exited. Used both for the running/stopped status
/// (non-empty vs. empty) and, by the overlay (`overlay::update_overlay_position`),
/// to find the actual game window without a name-matching guess.
#[cfg(windows)]
pub fn job_process_ids(job: GameJob) -> Vec<u32> {
    // `JOBOBJECT_BASIC_PROCESS_ID_LIST` is a variable-length struct -- its
    // trailing `ProcessIdList: [usize; 1]` is however many entries actually
    // fit in the buffer Windows is given, not really length 1. 64 is
    // comfortably more than any real launcher-stub-plus-game tree needs, so
    // a fixed, over-allocated buffer avoids the usual two-call "ask for the
    // required size first" dance.
    const MAX_PIDS: usize = 64;
    #[repr(C)]
    struct Buffer {
        header: JOBOBJECT_BASIC_PROCESS_ID_LIST,
        extra: [usize; MAX_PIDS - 1],
    }
    let mut buffer: Buffer = unsafe { std::mem::zeroed() };

    let result = unsafe {
        QueryInformationJobObject(
            Some(job.0),
            JobObjectBasicProcessIdList,
            &mut buffer as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<Buffer>() as u32,
            None,
        )
    };
    if let Err(error) = result {
        tracing::warn!(%error, "job_process_ids: QueryInformationJobObject failed");
        return Vec::new();
    }

    let count = (buffer.header.NumberOfProcessIdsInList as usize).min(MAX_PIDS);
    let ids_ptr = &buffer.header.ProcessIdList as *const usize;
    (0..count).map(|i| unsafe { *ids_ptr.add(i) as u32 }).collect()
}

#[cfg(not(windows))]
pub fn job_process_ids(_job: GameJob) -> Vec<u32> {
    Vec::new()
}

/// Explicitly closes a job handle -- only called when a new launch is about
/// to replace a previous one in shared state (see `launch_game`); never
/// called from anywhere in this process's normal shutdown path (that's the
/// whole point of `create_kill_on_close_job`'s "leaked on purpose" design).
#[cfg(windows)]
pub fn close_job(job: GameJob) {
    unsafe {
        let _ = CloseHandle(job.0);
    }
}

#[cfg(not(windows))]
pub fn close_job(_job: GameJob) {}

/// A spawned game process, still owned so its exit can be awaited (see
/// `LaunchedProcess::wait_for_exit`) — that's what lets the UI show "has
/// salido de la partida" and auto-leave the room instead of the process
/// just running unobserved once launched.
pub struct LaunchedProcess {
    pub pid: u32,
    /// See `create_kill_on_close_job` -- `None` if the job couldn't be set
    /// up (non-fatal; falls back to the older name-matching behavior).
    pub job: Option<GameJob>,
    child: Child,
}

impl LaunchedProcess {
    /// Resolves once the game process exits, however it exits (normal quit,
    /// crash, or killed externally) — the caller doesn't need to know which.
    pub async fn wait_for_exit(mut self) {
        let _ = self.child.wait().await;
    }
}

/// Spawns the target game executable detached from the launcher's own
/// lifecycle: closing the launcher does not kill the game.
pub fn launch(
    executable_path: &Path,
    args: &[String],
    working_dir: Option<&Path>,
) -> Result<LaunchedProcess, LauncherError> {
    if !executable_path.is_file() {
        return Err(LauncherError::NotFound(executable_path.to_path_buf()));
    }

    let mut command = Command::new(executable_path);
    command.args(args);
    if let Some(dir) = working_dir {
        command.current_dir(dir);
    } else if let Some(parent) = executable_path.parent() {
        command.current_dir(parent);
    }

    let child = command.spawn().map_err(|source| LauncherError::Spawn {
        path: executable_path.to_path_buf(),
        source,
    })?;
    let pid = child.id().ok_or_else(|| LauncherError::Spawn {
        path: executable_path.to_path_buf(),
        source: std::io::Error::other("process exited before its PID could be read"),
    })?;
    let job = create_kill_on_close_job(&child);

    Ok(LaunchedProcess { pid, job, child })
}

/// Checks whether any currently running process' image name matches
/// `executable_path`'s filename.
///
/// Originally this also opened each name-matching process
/// (`OpenProcess`/`QueryFullProcessImageNameW`) and required its full image
/// path to equal `executable_path` exactly, mirroring the identity check the
/// daemon does for its pipe clients (`daemon/src/security.rs`'s
/// `verify_client_process`) -- appropriate there since that's a real
/// security boundary, but wrong here: it went permanently blind for exactly
/// the case `launch_game`'s own `MIN_REAL_SESSION` comment already
/// documents ("many real-world game installs, Battle.net-wrapped Warcraft
/// III among them, run the launched .exe only as a stub that spawns the
/// actual game process"). The stub is what's configured/launched; the real,
/// still-running game process lives at a *different* path, so the strict
/// check reported "detenido" while the user was actively in a match.
/// Matching on the filename alone is what actually holds across that
/// indirection -- the same tolerance the exit side already needed.
#[cfg(windows)]
pub fn is_process_running_at(executable_path: &Path) -> bool {
    let Some(target_name) = executable_path.file_name().and_then(|n| n.to_str()) else {
        tracing::warn!(path = %executable_path.display(), "is_process_running_at: configured path has no filename component");
        return false;
    };

    let snapshot = match unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) } {
        Ok(handle) => handle,
        Err(error) => {
            tracing::warn!(%error, "is_process_running_at: CreateToolhelp32Snapshot failed");
            return false;
        }
    };

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    // Only collected for the miss-diagnostic log below -- cheap relative to
    // the snapshot walk itself, and this function runs on a 2s poll, not a
    // hot path.
    let mut scanned_names: Vec<String> = Vec::new();
    let mut found = false;
    let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while has_entry {
        let name_end = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        let exe_name = String::from_utf16_lossy(&entry.szExeFile[..name_end]);
        if exe_name.eq_ignore_ascii_case(target_name) {
            found = true;
            break;
        }
        scanned_names.push(exe_name);
        has_entry = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }

    unsafe {
        let _ = CloseHandle(snapshot);
    }

    if !found {
        // At `debug`, not `warn`: "not running" is the overwhelmingly
        // common, expected outcome of a 2s poll, so logging the full
        // process table on every miss would be constant noise. Instead,
        // surface just the process names that look game-related (share a
        // word with the target filename, e.g. "warcraft"/"war3") -- if the
        // real process is running under a name this filename-match doesn't
        // recognize, it should show up here, giving a *reported* false
        // negative something concrete to look at instead of having to be
        // guessed at again.
        let target_lower = target_name.to_ascii_lowercase();
        let target_words: Vec<&str> = target_lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            // "exe"/"dll" excluded on top of the length filter -- almost
            // every scanned process ends in ".exe", so without this every
            // single one would count as a "near miss" and defeat the point.
            .filter(|w| w.len() >= 3 && *w != "exe" && *w != "dll")
            .collect();
        let near_misses: Vec<&String> = scanned_names
            .iter()
            .filter(|name| {
                let lower = name.to_ascii_lowercase();
                target_words.iter().any(|w| lower.contains(w))
            })
            .collect();
        tracing::debug!(
            target_name,
            scanned_count = scanned_names.len(),
            ?near_misses,
            "is_process_running_at: no match"
        );
    }

    found
}

#[cfg(not(windows))]
pub fn is_process_running_at(_executable_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn missing_executable_is_reported_before_spawning() {
        let missing = PathBuf::from("Z:/definitely/not/a/real/path/game.exe");
        match launch(&missing, &[], None) {
            Err(LauncherError::NotFound(_)) => {}
            _ => panic!("expected LauncherError::NotFound"),
        }
    }
}
