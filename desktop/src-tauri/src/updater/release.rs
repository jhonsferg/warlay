//! Reading a GitHub release: which version it is, which file is the Windows
//! installer, and what that file's SHA-256 should be. Pure (no network, no
//! files) so every rule can be unit tested.

use serde::Serialize;
use serde_json::Value;

/// Releases are only ever trusted from this repository.
pub const REPO: &str = "jhonsferg/warlay";

/// Where the installer and checksums of a release may be downloaded from.
const DOWNLOAD_PREFIX: &str = "https://github.com/jhonsferg/warlay/releases/download/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    /// Without the leading `v`.
    pub version: String,
    pub notes: String,
    pub published_at: String,
    pub page_url: String,
    pub installer: Option<Asset>,
    pub checksums: Option<Asset>,
}

/// `1.2.3`, `v1.2.3`; anything else (pre-releases, junk) is not a version.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim().trim_start_matches('v');
    let mut parts = text.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Whether `latest` is a newer version than `current`. Unparseable on either
/// side means no update is offered: never push a doubtful install.
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    }
}

/// Only files published by this repository's releases are ever downloaded.
pub fn is_trusted_url(url: &str) -> bool {
    url.starts_with(DOWNLOAD_PREFIX) && !url.contains("..") && !url.contains('@')
}

fn asset_from(value: &Value) -> Option<Asset> {
    let url = value.get("browser_download_url")?.as_str()?;
    if !is_trusted_url(url) {
        return None;
    }
    Some(Asset {
        name: value.get("name")?.as_str()?.to_string(),
        url: url.to_string(),
        size: value.get("size").and_then(Value::as_u64).unwrap_or(0),
    })
}

/// The releases page, used when the release does not name a better one.
pub fn releases_page() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

/// Reads the JSON of `GET /repos/{REPO}/releases/latest`. Drafts and
/// pre-releases are refused.
pub fn from_github_json(json: &Value) -> Result<ReleaseInfo, String> {
    if json.get("draft").and_then(Value::as_bool).unwrap_or(false)
        || json.get("prerelease").and_then(Value::as_bool).unwrap_or(false)
    {
        return Err("el release más reciente es un borrador o una versión previa".to_string());
    }
    let tag = json
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or("el release no tiene etiqueta de versión")?;
    parse_version(tag).ok_or_else(|| format!("la etiqueta '{tag}' no es una versión"))?;

    let assets: Vec<Asset> = json
        .get("assets")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(asset_from).collect())
        .unwrap_or_default();

    let page_url = json
        .get("html_url")
        .and_then(Value::as_str)
        .filter(|u| u.starts_with(&format!("https://github.com/{REPO}/releases")))
        .map(str::to_string)
        .unwrap_or_else(releases_page);

    Ok(ReleaseInfo {
        version: tag.trim_start_matches('v').to_string(),
        notes: json.get("body").and_then(Value::as_str).unwrap_or_default().to_string(),
        published_at: json
            .get("published_at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        page_url,
        installer: assets
            .iter()
            .find(|a| a.name.starts_with("warlay-desktop-v") && a.name.ends_with("-windows-x64-setup.exe"))
            .cloned(),
        checksums: assets.iter().find(|a| a.name == "SHA256SUMS.txt").cloned(),
    })
}

/// The expected SHA-256 (lowercase hex) of `file` in a `sha256sum` listing
/// (`<hash>  <name>` or `<hash> *<name>` per line).
pub fn checksum_for(listing: &str, file: &str) -> Option<String> {
    listing.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        let valid = hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit());
        (valid && name == file).then(|| hash.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SETUP: &str = "warlay-desktop-v0.10.0-windows-x64-setup.exe";
    const HASH: &str = "4f2d5b0e8a1c3d7e9f60123456789abcdef0123456789abcdef0123456789abc";

    fn url(name: &str) -> String {
        format!("{DOWNLOAD_PREFIX}v0.10.0/{name}")
    }

    fn release() -> Value {
        json!({
            "tag_name": "v0.10.0",
            "draft": false,
            "prerelease": false,
            "body": "## Changes\n- algo",
            "published_at": "2026-11-09T10:00:00Z",
            "html_url": "https://github.com/jhonsferg/warlay/releases/tag/v0.10.0",
            "assets": [
                {"name": SETUP, "browser_download_url": url(SETUP), "size": 40_000_000},
                {"name": "warlay-desktop-v0.10.0-windows-x64-portable.zip", "browser_download_url": url("p.zip"), "size": 1},
                {"name": "SHA256SUMS.txt", "browser_download_url": url("SHA256SUMS.txt"), "size": 900},
            ],
        })
    }

    #[test]
    fn versions_parse_with_or_without_the_v() {
        assert_eq!(parse_version("0.9.0"), Some((0, 9, 0)));
        assert_eq!(parse_version("v1.20.3"), Some((1, 20, 3)));
        assert_eq!(parse_version(" v2.0.0 "), Some((2, 0, 0)));
    }

    #[test]
    fn anything_that_is_not_a_plain_version_is_rejected() {
        for bad in ["", "1.2", "1.2.3.4", "1.2.x", "v", "1.2.3-beta", "latest"] {
            assert_eq!(parse_version(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn newer_means_strictly_greater_by_number_not_by_text() {
        assert!(is_newer("0.9.0", "0.10.0"), "0.10 is above 0.9");
        assert!(is_newer("0.9.9", "1.0.0"));
        assert!(is_newer("v0.9.0", "0.9.1"));
        assert!(!is_newer("0.9.0", "0.9.0"));
        assert!(!is_newer("0.10.0", "0.9.0"), "never a downgrade");
    }

    #[test]
    fn a_doubtful_version_never_offers_an_update() {
        assert!(!is_newer("dev", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.1.0-rc1"));
    }

    #[test]
    fn only_this_repositorys_release_files_are_trusted() {
        assert!(is_trusted_url(&url(SETUP)));
        assert!(!is_trusted_url(
            "https://evil.example/jhonsferg/warlay/releases/download/x"
        ));
        assert!(!is_trusted_url(
            "http://github.com/jhonsferg/warlay/releases/download/x"
        ));
        assert!(!is_trusted_url("https://github.com/other/warlay/releases/download/x"));
        assert!(!is_trusted_url(
            "https://github.com/jhonsferg/warlay/releases/download/../../x"
        ));
        assert!(!is_trusted_url(
            "https://github.com/jhonsferg/warlay/releases/download/@evil.example/x"
        ));
    }

    #[test]
    fn a_release_gives_the_installer_and_the_checksums() {
        let info = from_github_json(&release()).unwrap();
        assert_eq!(info.version, "0.10.0");
        assert_eq!(info.published_at, "2026-11-09T10:00:00Z");
        assert!(info.notes.contains("algo"));
        let installer = info.installer.unwrap();
        assert_eq!(installer.name, SETUP);
        assert_eq!(installer.size, 40_000_000);
        assert_eq!(info.checksums.unwrap().name, "SHA256SUMS.txt");
        assert!(info.page_url.ends_with("/tag/v0.10.0"));
    }

    #[test]
    fn the_portable_zip_is_not_taken_for_the_installer() {
        let mut json = release();
        json["assets"] =
            json!([{"name": "warlay-desktop-v0.10.0-windows-x64-portable.zip", "browser_download_url": url("p.zip")}]);
        assert_eq!(from_github_json(&json).unwrap().installer, None);
    }

    #[test]
    fn assets_from_untrusted_hosts_are_ignored() {
        let mut json = release();
        json["assets"] = json!([{"name": SETUP, "browser_download_url": "https://evil.example/setup.exe", "size": 1}]);
        assert_eq!(from_github_json(&json).unwrap().installer, None);
    }

    #[test]
    fn drafts_prereleases_and_bad_tags_are_refused() {
        for (key, value) in [
            ("draft", json!(true)),
            ("prerelease", json!(true)),
            ("tag_name", json!("nightly")),
        ] {
            let mut json = release();
            json[key] = value;
            assert!(from_github_json(&json).is_err(), "{key}");
        }
        assert!(from_github_json(&json!({})).is_err());
    }

    #[test]
    fn a_foreign_page_url_falls_back_to_the_releases_page() {
        let mut json = release();
        json["html_url"] = json!("https://evil.example/");
        assert_eq!(from_github_json(&json).unwrap().page_url, releases_page());
    }

    #[test]
    fn the_checksum_of_a_file_is_found_in_the_listing() {
        let other = "a".repeat(64);
        let listing = format!("{HASH}  {SETUP}\n{other}  other.zip\n");
        assert_eq!(checksum_for(&listing, SETUP).as_deref(), Some(HASH));
        assert_eq!(checksum_for(&listing, "other.zip").as_deref(), Some(other.as_str()));
        assert_eq!(checksum_for(&listing, "missing.exe"), None);
    }

    #[test]
    fn checksum_listings_in_binary_mode_and_uppercase_still_match() {
        let listing = format!("{}  *{SETUP}\r\n", HASH.to_ascii_uppercase());
        assert_eq!(checksum_for(&listing, SETUP).as_deref(), Some(HASH));
    }

    #[test]
    fn a_malformed_checksum_line_is_never_accepted() {
        assert_eq!(checksum_for(&format!("abc123  {SETUP}"), SETUP), None);
        assert_eq!(checksum_for(&format!("{}  {SETUP}", "z".repeat(64)), SETUP), None);
    }
}
