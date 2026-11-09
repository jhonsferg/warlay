//! Keeps Windows from slowing Warlay down while a game has the focus.
//!
//! Warlay usually runs behind a fullscreen Warcraft III. Windows may then
//! treat it as a background process: lower its CPU speed (mostly on battery)
//! and stretch its timers to the system tick (about 15.6 ms). Both hurt
//! exactly what Warlay does: QUIC loss recovery and pacing run on timers, and
//! voice frames need to be handled the moment they arrive.
//!
//! Failing here is never fatal: the app works the same, only less promptly.

#[cfg(windows)]
pub fn keep_responsive() {
    use tracing::{info, warn};
    use windows::Win32::Media::timeBeginPeriod;
    use windows::Win32::System::Threading::{
        GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
        PROCESS_POWER_THROTTLING_STATE,
    };

    // A zero state mask with both bits in the control mask opts out of both
    // behaviours explicitly (instead of leaving them to the system's choice).
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED | PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
        StateMask: 0,
    };
    // SAFETY: `state` is a valid, fully initialised struct that outlives the
    // call, and the size passed is its real size.
    let throttling = unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            std::ptr::addr_of!(state).cast(),
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
    };
    match throttling {
        Ok(()) => info!("power throttling and timer coalescing turned off for this process"),
        Err(error) => warn!(%error, "could not turn off power throttling"),
    }

    // 1 ms system timer while Warlay runs, so timers fire on time. SAFETY: a
    // plain call with no pointers; it is undone by the OS when the process ends.
    let resolution = unsafe { timeBeginPeriod(1) };
    if resolution != 0 {
        warn!(resolution, "could not request a 1 ms timer resolution");
    }
}

#[cfg(not(windows))]
pub fn keep_responsive() {}
