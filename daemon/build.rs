fn main() {
    #[cfg(windows)]
    embed_windows_resources();
}

/// Without this, wnetd.exe carries no version info or icon at all, so
/// Task Manager (and any UAC/Firewall prompt it triggers, since it runs
/// elevated) shows it as a bare, unbranded exe -- indistinguishable from
/// any other unknown background process. Mirrors what Tauri's own build
/// already does for warlay.exe via tauri-build/tauri-winres.
///
/// Note for the UAC prompt specifically: Windows only trusts FileDescription
/// enough to headline an *unsigned* binary's consent dialog with it on some
/// builds -- on others it falls back to showing the raw filename instead,
/// precisely because an unsigned exe's own self-reported metadata can't be
/// verified. Setting FileDescription here is still correct and is what
/// Explorer/Task Manager/property sheets show either way; only real code
/// signing removes the "or the raw filename" fallback from the UAC path.
///
/// The embedded manifest's `requireAdministrator` is what makes elevation
/// unconditional regardless of *how* wnetd.exe is started: Explorer (a
/// double-click) reads this manifest and shows the UAC prompt on its own,
/// and any plain CreateProcess-based launch (warlay.exe spawning it
/// without going through ShellExecute) fails outright with
/// ERROR_ELEVATION_REQUIRED instead of silently starting unprivileged --
/// there is no path left where this binary just runs as a normal user.
/// warlay.exe's own launch still explicitly requests the "runas" verb
/// (see commands/daemon.rs) on top of this, but the manifest is the real
/// guarantee: it doesn't depend on every caller remembering to ask.
#[cfg(windows)]
fn embed_windows_resources() {
    const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#;

    let mut res = tauri_winres::WindowsResource::new();
    res.set("FileDescription", "Warlay Network Agent");
    res.set("ProductName", "Warlay");
    res.set("OriginalFilename", "wnetd.exe");
    res.set_icon("../desktop/src-tauri/icons/icon.ico");
    res.set_manifest(MANIFEST);
    res.compile().expect("failed to embed Windows resources into wnetd.exe");
}
