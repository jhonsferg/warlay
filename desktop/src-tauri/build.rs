fn main() {
    // Tauri embeds a default manifest (`asInvoker`: no elevation) unless the
    // build script hands it one, and a PE carries exactly ONE manifest. This
    // is where "the agent lives inside this binary" becomes true: without
    // `requireAdministrator` here, warlay.exe would start unelevated and the
    // agent instance it spawns would silently fail to open the raw LAN
    // capture socket, only revealing itself on the first game - the exact
    // failure mode documented in `desktop/src-tauri/build.rs`.
    //
    // The manifest is a separate file rather than an inline string because it
    // must stay comparable with Tauri's default: the Common Controls v6
    // dependency it carries is required by Tauri's dialog APIs (dropping it
    // changes how every native dialog renders), so only the execution level
    // differs from upstream's `windows-app-manifest.xml`. See the notes in
    // that file before editing it.
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("windows-app-manifest.xml"));
    let attributes = tauri_build::Attributes::new().windows_attributes(windows);

    // Unit tests that use Tauri's mock runtime need Common Controls v6 in the
    // test executable (without it Windows refuses to load it:
    // STATUS_ENTRYPOINT_NOT_FOUND), but must NOT demand elevation or
    // `cargo test` fails with os error 740. Cargo's `rustc-link-arg-tests`
    // does not reach a library's unit-test harness, and a plain
    // `rustc-link-arg` would also hit warlay.exe (a second manifest), so this
    // is opt-in and only for test runs:
    //   WARLAY_TEST_MANIFEST=1 cargo test -p desktop
    println!("cargo:rerun-if-env-changed=WARLAY_TEST_MANIFEST");
    if std::env::var("WARLAY_TEST_MANIFEST").as_deref() == Ok("1")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("windows-test-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attributes).expect("failed to run build script");
}
