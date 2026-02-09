use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{ActionbookError, Result};

const GITHUB_REPO: &str = "actionbook/actionbook";
const RELEASE_TAG_PREFIX: &str = "actionbook-extension-v";
const USER_AGENT: &str = concat!("actionbook-cli/", env!("CARGO_PKG_VERSION"));
/// Maximum download size for the extension zip (10 MB compressed)
const MAX_DOWNLOAD_SIZE: usize = 10 * 1024 * 1024;
/// Maximum total uncompressed size (50 MB — zip bomb protection)
const MAX_UNCOMPRESSED_SIZE: u64 = 50 * 1024 * 1024;
/// Allowed download hosts (GitHub asset CDN)
const ALLOWED_DOWNLOAD_HOSTS: &[&str] = &["github.com", "githubusercontent.com"];

/// Returns the extension install directory: ~/.config/actionbook/extension/
pub fn extension_dir() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().ok_or_else(|| {
        ActionbookError::ExtensionError(
            "Could not determine config directory".to_string(),
        )
    })?;
    Ok(config_dir.join("actionbook").join("extension"))
}

/// Check if the extension is installed (manifest.json exists on disk)
pub fn is_installed() -> bool {
    extension_dir()
        .map(|dir| dir.join("manifest.json").exists())
        .unwrap_or(false)
}

/// Read the installed extension version from the on-disk manifest.json
pub fn installed_version() -> Option<String> {
    let dir = extension_dir().ok()?;
    let manifest_path = dir.join("manifest.json");
    let content = fs::read_to_string(manifest_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    parsed
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Remove the installed extension directory
pub fn uninstall() -> Result<()> {
    let dir = extension_dir()?;
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| {
            ActionbookError::ExtensionError(format!(
                "Failed to remove {}: {}",
                dir.display(),
                e
            ))
        })?;
    }
    Ok(())
}

/// Build a reqwest client with HTTPS-only and timeouts.
fn build_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| {
            ActionbookError::ExtensionError(format!(
                "Failed to create HTTP client: {}",
                e
            ))
        })
}

/// Download the latest extension release from GitHub and install it.
///
/// Returns the installed version string on success.
/// If `force` is false and the extension is already installed at the same or newer
/// version, returns an error.
pub async fn download_and_install(force: bool) -> Result<String> {
    let dir = extension_dir()?;

    // Fetch latest extension release info from GitHub
    let (version, asset_url) = fetch_latest_release().await?;

    if is_installed() && !force {
        let current = installed_version().unwrap_or_default();
        let current_semver = semver::Version::parse(&current).map_err(|e| {
            ActionbookError::ExtensionError(format!(
                "Installed version '{}' is not valid semver: {}. Use --force to reinstall",
                current, e
            ))
        })?;
        let latest_semver = semver::Version::parse(&version).map_err(|e| {
            ActionbookError::ExtensionError(format!(
                "Release version '{}' is not valid semver: {}",
                version, e
            ))
        })?;

        if current_semver >= latest_semver {
            return Err(ActionbookError::ExtensionError(format!(
                "Extension v{} is already installed at {} (latest: v{}). Use --force to reinstall",
                current,
                dir.display(),
                version
            )));
        }
    }

    // Download the zip asset
    let zip_bytes = download_asset(&asset_url).await?;

    // Clean existing install if present
    match fs::remove_dir_all(&dir) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(ActionbookError::ExtensionError(format!(
                "Failed to clean existing install at {}: {}",
                dir.display(),
                e
            )));
        }
    }

    // Extract zip to extension dir
    extract_zip(&zip_bytes, &dir)?;

    // Verify extracted manifest version matches the release
    let extracted_version = installed_version().ok_or_else(|| {
        ActionbookError::ExtensionError(
            "Extraction succeeded but manifest.json is missing or unreadable".to_string(),
        )
    })?;
    if extracted_version != version {
        return Err(ActionbookError::ExtensionError(format!(
            "Version mismatch after extraction: expected v{}, got v{}. Release may be corrupted",
            version, extracted_version
        )));
    }

    Ok(version)
}

/// Fetch the latest actionbook-extension release from GitHub API.
///
/// Returns (version, asset_download_url).
async fn fetch_latest_release() -> Result<(String, String)> {
    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page=20",
        GITHUB_REPO
    );

    let client = build_http_client()?;

    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            ActionbookError::ExtensionError(format!(
                "Failed to fetch releases from GitHub: {}. Check your network connection",
                e
            ))
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ActionbookError::ExtensionError(format!(
            "GitHub API returned {}: {}. If rate-limited, try again later or download manually from https://github.com/{}/releases",
            status, body, GITHUB_REPO
        )));
    }

    let releases: Vec<serde_json::Value> = resp.json().await.map_err(|e| {
        ActionbookError::ExtensionError(format!(
            "Failed to parse GitHub releases response: {}",
            e
        ))
    })?;

    // Find the latest release with an actionbook-extension-v* tag
    for release in &releases {
        let tag = release
            .get("tag_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !tag.starts_with(RELEASE_TAG_PREFIX) {
            continue;
        }

        let version = tag.trim_start_matches(RELEASE_TAG_PREFIX).to_string();
        if version.is_empty() {
            continue;
        }

        // Find the .zip asset with exact name match
        let expected_asset_name = format!("actionbook-extension-v{}.zip", version);
        let assets = release
            .get("assets")
            .and_then(|v| v.as_array());

        for asset in assets.into_iter().flatten() {
            let name = asset
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if name == expected_asset_name {
                let download_url = asset
                    .get("browser_download_url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ActionbookError::ExtensionError(format!(
                            "Release asset '{}' is missing download URL",
                            name
                        ))
                    })?;

                // Validate download URL host
                validate_download_url(download_url)?;

                return Ok((version, download_url.to_string()));
            }
        }
    }

    Err(ActionbookError::ExtensionError(format!(
        "No extension release found. Check https://github.com/{}/releases for available versions",
        GITHUB_REPO
    )))
}

/// Validate that a download URL points to an allowed GitHub host.
fn validate_download_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).map_err(|e| {
        ActionbookError::ExtensionError(format!("Invalid download URL: {}", e))
    })?;

    if parsed.scheme() != "https" {
        return Err(ActionbookError::ExtensionError(
            "Download URL must use HTTPS".to_string(),
        ));
    }

    let host = parsed.host_str().unwrap_or("");
    if !ALLOWED_DOWNLOAD_HOSTS.iter().any(|&allowed| host == allowed || host.ends_with(&format!(".{}", allowed))) {
        return Err(ActionbookError::ExtensionError(format!(
            "Download URL host '{}' is not allowed (expected GitHub)",
            host
        )));
    }

    Ok(())
}

/// Download a file from a URL, returning the bytes.
///
/// Enforces a maximum download size to prevent resource exhaustion.
async fn download_asset(url: &str) -> Result<Vec<u8>> {
    let client = build_http_client()?;

    let resp = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| {
            ActionbookError::ExtensionError(format!(
                "Failed to download extension: {}",
                e
            ))
        })?;

    if !resp.status().is_success() {
        return Err(ActionbookError::ExtensionError(format!(
            "Download failed with status {}. Try downloading manually from {}",
            resp.status(),
            url
        )));
    }

    // Check content-length if available
    if let Some(content_length) = resp.content_length() {
        if content_length > MAX_DOWNLOAD_SIZE as u64 {
            return Err(ActionbookError::ExtensionError(format!(
                "Extension download too large ({} bytes, max {} bytes). This may indicate a corrupted release",
                content_length, MAX_DOWNLOAD_SIZE
            )));
        }
    }

    let bytes = resp.bytes().await.map_err(|e| {
        ActionbookError::ExtensionError(format!(
            "Failed to read download response: {}",
            e
        ))
    })?;

    if bytes.len() > MAX_DOWNLOAD_SIZE {
        return Err(ActionbookError::ExtensionError(format!(
            "Extension download too large ({} bytes, max {} bytes)",
            bytes.len(),
            MAX_DOWNLOAD_SIZE
        )));
    }

    Ok(bytes.to_vec())
}

/// Extract a zip archive to a target directory.
///
/// Safety measures:
/// - Zip-slip protection via `enclosed_name()` (rejects `..` path components)
/// - Per-entry and total uncompressed size limits (zip bomb protection)
pub fn extract_zip(bytes: &[u8], target_dir: &Path) -> Result<()> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| {
        ActionbookError::ExtensionError(format!(
            "The downloaded extension file appears corrupted: {}",
            e
        ))
    })?;

    fs::create_dir_all(target_dir).map_err(|e| {
        ActionbookError::ExtensionError(format!(
            "Failed to create directory {}: {}",
            target_dir.display(),
            e
        ))
    })?;

    let mut total_uncompressed: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| {
            ActionbookError::ExtensionError(format!(
                "Failed to read zip entry {}: {}",
                i, e
            ))
        })?;

        // Zip bomb protection: check uncompressed size
        total_uncompressed = total_uncompressed.saturating_add(file.size());
        if total_uncompressed > MAX_UNCOMPRESSED_SIZE {
            return Err(ActionbookError::ExtensionError(format!(
                "Total uncompressed size exceeds {} bytes (zip bomb protection)",
                MAX_UNCOMPRESSED_SIZE
            )));
        }

        // enclosed_name() returns None for entries with path traversal (e.g. "../")
        let entry_path = file
            .enclosed_name()
            .ok_or_else(|| {
                ActionbookError::ExtensionError(format!(
                    "Zip entry '{}' has an unsafe path — possible zip-slip attack",
                    file.name()
                ))
            })?
            .to_path_buf();

        let out_path = target_dir.join(&entry_path);

        if file.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| {
                ActionbookError::ExtensionError(format!(
                    "Failed to create directory {}: {}",
                    out_path.display(),
                    e
                ))
            })?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    ActionbookError::ExtensionError(format!(
                        "Failed to create directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }

            let mut buf = Vec::new();
            file.read_to_end(&mut buf).map_err(|e| {
                ActionbookError::ExtensionError(format!(
                    "Failed to read zip entry {}: {}",
                    entry_path.display(),
                    e
                ))
            })?;

            fs::write(&out_path, &buf).map_err(|e| {
                ActionbookError::ExtensionError(format!(
                    "Failed to write {}: {}",
                    out_path.display(),
                    e
                ))
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_dir_is_under_config() {
        let dir = extension_dir().expect("should resolve config dir");
        assert!(dir.ends_with("actionbook/extension"));
    }

    #[test]
    fn test_extract_zip() {
        let tmp = tempfile::tempdir().expect("should create temp dir");
        let target = tmp.path().join("ext");

        // Create a minimal zip in memory
        let buf = Vec::new();
        let cursor = std::io::Cursor::new(buf);
        let mut writer = zip::ZipWriter::new(cursor);

        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        writer
            .start_file("manifest.json", options)
            .expect("start_file");
        std::io::Write::write_all(
            &mut writer,
            b"{\"manifest_version\":3,\"name\":\"Test\",\"version\":\"1.0.0\"}",
        )
        .expect("write");

        writer
            .start_file("background.js", options)
            .expect("start_file");
        std::io::Write::write_all(&mut writer, b"// background")
            .expect("write");

        writer.add_directory("icons", options).expect("add_directory");

        writer
            .start_file("icons/icon-16.png", options)
            .expect("start_file");
        std::io::Write::write_all(&mut writer, b"fake-png-data")
            .expect("write");

        let result = writer.finish().expect("finish");
        let zip_bytes = result.into_inner();

        // Extract and verify
        extract_zip(&zip_bytes, &target).expect("extract should succeed");

        assert!(target.join("manifest.json").exists());
        assert!(target.join("background.js").exists());
        assert!(target.join("icons/icon-16.png").exists());

        let manifest = fs::read_to_string(target.join("manifest.json")).unwrap();
        assert!(manifest.contains("\"version\":\"1.0.0\""));
    }

    #[test]
    fn test_extract_zip_slip_protection() {
        let tmp = tempfile::tempdir().expect("should create temp dir");
        let target = tmp.path().join("ext");
        fs::create_dir_all(&target).unwrap();

        let buf = Vec::new();
        let cursor = std::io::Cursor::new(buf);
        let mut writer = zip::ZipWriter::new(cursor);

        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        writer
            .start_file("../../../etc/passwd", options)
            .expect("start_file");
        std::io::Write::write_all(&mut writer, b"malicious content")
            .expect("write");

        let result = writer.finish().expect("finish");
        let zip_bytes = result.into_inner();

        let err = extract_zip(&zip_bytes, &target);
        assert!(err.is_err(), "should reject path traversal");
    }

    #[test]
    fn test_extract_zip_corrupted() {
        let tmp = tempfile::tempdir().expect("should create temp dir");
        let target = tmp.path().join("ext");

        let result = extract_zip(b"this is not a zip", &target);
        assert!(result.is_err(), "should reject corrupted data");
    }

    #[test]
    fn test_validate_download_url_accepts_github() {
        assert!(validate_download_url(
            "https://github.com/actionbook/actionbook/releases/download/v0.2.0/ext.zip"
        ).is_ok());
        assert!(validate_download_url(
            "https://objects.githubusercontent.com/some-path/ext.zip"
        ).is_ok());
    }

    #[test]
    fn test_validate_download_url_rejects_non_github() {
        assert!(validate_download_url("https://evil.com/ext.zip").is_err());
        assert!(validate_download_url("http://github.com/ext.zip").is_err());
        assert!(validate_download_url("https://not-github.com/ext.zip").is_err());
    }
}
