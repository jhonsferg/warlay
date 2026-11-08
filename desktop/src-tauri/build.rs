/// What identifies this build in "Acerca de": the short commit and the moment
/// it was built (the release date of a CI build).
fn stamp_build() {
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    let commit = std::env::var("GITHUB_SHA")
        .ok()
        .filter(|sha| sha.len() >= 7)
        .map(|sha| sha[..7].to_string())
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "--short=7", "HEAD"])
                .output()
                .ok()
                .filter(|out| out.status.success())
                .and_then(|out| String::from_utf8(out.stdout).ok())
                .map(|text| text.trim().to_string())
        })
        .filter(|commit| !commit.is_empty())
        .unwrap_or_else(|| "local".to_string());
    let built = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    println!("cargo:rustc-env=WARLAY_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=WARLAY_BUILD_UNIX={built}");
}

fn main() {
    stamp_build();
    // Tauri embeds a default manifest (`asInvoker`: no elevation) unless the
    // build script hands it one, and a PE carries exactly ONE manifest. This
    // is what lets the client capture LAN game broadcasts: without
    // `requireAdministrator` here, warlay.exe would start unelevated and its
    // raw LAN capture socket would silently fail to open, only revealing
    // itself on the first game.
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
        let manifest =
            std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("windows-test-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attributes).expect("failed to run build script");
}
