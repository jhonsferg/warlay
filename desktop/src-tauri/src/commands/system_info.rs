use crate::core::dto::SystemInfoDto;

/// Read-only hardware snapshot shown in Ajustes next to the "Motor gráfico"
/// selector — purely informational (which GPU/how much VRAM/how many CPU
/// cores are actually present), so the user has something to go on when
/// deciding whether Direct3D 11 or OpenGL2 is worth trying. Nothing here
/// changes any setting automatically; auto-picking a graphics API from this
/// data would risk "helpfully" selecting one that crashes on a given
/// driver/patch combination (see the `-graphicsapi` compatibility notes on
/// `SettingsView.vue`).
#[tauri::command]
pub fn get_system_info() -> SystemInfoDto {
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);

    #[cfg(windows)]
    let (total_ram_mb, gpu_name, gpu_vram_mb) = {
        let ram = windows_impl::total_ram_mb();
        let (gpu, vram) = windows_impl::primary_gpu();
        (ram, gpu, vram)
    };
    #[cfg(not(windows))]
    let (total_ram_mb, gpu_name, gpu_vram_mb) = (0u64, None, None);

    SystemInfoDto {
        cpu_cores,
        total_ram_mb,
        gpu_name,
        gpu_vram_mb,
    }
}

#[cfg(windows)]
mod windows_impl {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    pub fn total_ram_mb() -> u64 {
        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        // SAFETY: `status` is a valid, correctly-sized buffer per the
        // Win32 API contract (`dwLength` set before the call, as required).
        match unsafe { GlobalMemoryStatusEx(&mut status) } {
            Ok(()) => status.ullTotalPhys / (1024 * 1024),
            Err(_) => 0,
        }
    }

    /// The first hardware adapter DXGI reports — good enough for a "here's
    /// what we detected" hint; a full multi-GPU picker is out of scope for
    /// what's just an informational aid next to the graphics-api dropdown.
    pub fn primary_gpu() -> (Option<String>, Option<u64>) {
        // SAFETY: `CreateDXGIFactory1` has no preconditions beyond being
        // called on a thread with COM usable in MTA-compatible mode, which
        // holds for a plain Tauri command handler.
        let factory: IDXGIFactory1 = match unsafe { CreateDXGIFactory1() } {
            Ok(f) => f,
            Err(_) => return (None, None),
        };
        // SAFETY: `factory` is a valid, just-created IDXGIFactory1; index 0
        // is the first enumerated adapter, an in-bounds call per the API.
        let adapter = match unsafe { factory.EnumAdapters1(0) } {
            Ok(a) => a,
            Err(_) => return (None, None),
        };
        // SAFETY: `adapter` is a valid IDXGIAdapter1 from the call above.
        let desc = match unsafe { adapter.GetDesc1() } {
            Ok(d) => d,
            Err(_) => return (None, None),
        };
        let name_len = desc
            .Description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(desc.Description.len());
        let name = String::from_utf16_lossy(&desc.Description[..name_len]);
        let vram_mb = (desc.DedicatedVideoMemory as u64) / (1024 * 1024);
        (Some(name), Some(vram_mb))
    }
}
