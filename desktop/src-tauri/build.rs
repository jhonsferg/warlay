fn main() {
    // Tauri embeds a default manifest (`asInvoker`: no elevation) unless the
    // build script hands it one, and a PE carries exactly ONE manifest. This
    // is where "the agent lives inside this binary" becomes true: without
    // `requireAdministrator` here, warlay.exe would start unelevated and the
    // daemon instance it spawns would silently fail to open the raw LAN
    // capture socket, only revealing itself on the first game - the exact
    // failure mode documented in daemon/build.rs.
    //
    // The manifest is a separate file rather than an inline string because it
    // must stay comparable with Tauri's default: the Common Controls v6
    // dependency it carries is required by Tauri's dialog APIs (dropping it
    // changes how every native dialog renders), so only the execution level
    // differs from upstream's `windows-app-manifest.xml`. See the notes in
    // that file before editing it.
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("windows-app-manifest.xml"));
    let attributes = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attributes).expect("failed to run build script");
}
