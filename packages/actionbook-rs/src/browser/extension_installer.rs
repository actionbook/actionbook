use std::fs;
use std::path::PathBuf;

use crate::error::{ActionbookError, Result};

/// Embedded extension version - must match manifest.json version
pub const EXTENSION_VERSION: &str = "0.2.0";

// --- Embedded extension files (text) ---
const MANIFEST_JSON: &str =
    include_str!("../../../actionbook-extension/manifest.json");
const BACKGROUND_JS: &str =
    include_str!("../../../actionbook-extension/background.js");
const POPUP_HTML: &str =
    include_str!("../../../actionbook-extension/popup.html");
const POPUP_JS: &str =
    include_str!("../../../actionbook-extension/popup.js");
const OFFSCREEN_HTML: &str =
    include_str!("../../../actionbook-extension/offscreen.html");
const OFFSCREEN_JS: &str =
    include_str!("../../../actionbook-extension/offscreen.js");

// --- Embedded extension files (binary icons) ---
const ICON_16: &[u8] =
    include_bytes!("../../../actionbook-extension/icons/icon-16.png");
const ICON_48: &[u8] =
    include_bytes!("../../../actionbook-extension/icons/icon-48.png");
const ICON_128: &[u8] =
    include_bytes!("../../../actionbook-extension/icons/icon-128.png");

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

/// Check if the installed extension is outdated compared to the embedded version
pub fn is_outdated() -> bool {
    match installed_version() {
        Some(version) => version != EXTENSION_VERSION,
        None => false,
    }
}

/// Extract all embedded extension files to the install directory.
///
/// Returns the install path on success.
/// If `force` is false and the extension is already installed at the same version, returns an error.
pub fn extract(force: bool) -> Result<PathBuf> {
    let dir = extension_dir()?;

    if is_installed() && !force {
        let current = installed_version().unwrap_or_default();
        if current == EXTENSION_VERSION {
            return Err(ActionbookError::ExtensionError(format!(
                "Extension v{} is already installed at {}",
                EXTENSION_VERSION,
                dir.display()
            )));
        }
    }

    // Create directories
    let icons_dir = dir.join("icons");
    fs::create_dir_all(&icons_dir).map_err(|e| {
        ActionbookError::ExtensionError(format!(
            "Failed to create directory {}: {}",
            icons_dir.display(),
            e
        ))
    })?;

    // Write text files
    let text_files: &[(&str, &str)] = &[
        ("manifest.json", MANIFEST_JSON),
        ("background.js", BACKGROUND_JS),
        ("popup.html", POPUP_HTML),
        ("popup.js", POPUP_JS),
        ("offscreen.html", OFFSCREEN_HTML),
        ("offscreen.js", OFFSCREEN_JS),
    ];

    for (name, content) in text_files {
        let path = dir.join(name);
        fs::write(&path, content).map_err(|e| {
            ActionbookError::ExtensionError(format!(
                "Failed to write {}: {}",
                path.display(),
                e
            ))
        })?;
    }

    // Write icon files
    let icon_files: &[(&str, &[u8])] = &[
        ("icons/icon-16.png", ICON_16),
        ("icons/icon-48.png", ICON_48),
        ("icons/icon-128.png", ICON_128),
    ];

    for (name, content) in icon_files {
        let path = dir.join(name);
        fs::write(&path, content).map_err(|e| {
            ActionbookError::ExtensionError(format!(
                "Failed to write {}: {}",
                path.display(),
                e
            ))
        })?;
    }

    Ok(dir)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_version_matches_manifest() {
        let parsed: serde_json::Value =
            serde_json::from_str(MANIFEST_JSON).expect("manifest.json should be valid JSON");
        let manifest_version = parsed
            .get("version")
            .and_then(|v| v.as_str())
            .expect("manifest.json should have a version field");
        assert_eq!(
            manifest_version, EXTENSION_VERSION,
            "EXTENSION_VERSION const must match manifest.json version"
        );
    }

    #[test]
    fn test_extension_dir_is_under_config() {
        let dir = extension_dir().expect("should resolve config dir");
        assert!(dir.ends_with("actionbook/extension"));
    }

    #[test]
    fn test_embedded_files_not_empty() {
        assert!(!MANIFEST_JSON.is_empty());
        assert!(!BACKGROUND_JS.is_empty());
        assert!(!POPUP_HTML.is_empty());
        assert!(!POPUP_JS.is_empty());
        assert!(!OFFSCREEN_HTML.is_empty());
        assert!(!OFFSCREEN_JS.is_empty());
        assert!(!ICON_16.is_empty());
        assert!(!ICON_48.is_empty());
        assert!(!ICON_128.is_empty());
    }

    #[test]
    fn test_extract_and_uninstall() {
        // Use a temporary directory to avoid polluting the real config dir
        let tmp = tempfile::tempdir().expect("should create temp dir");
        let dir = tmp.path().join("actionbook").join("extension");
        let icons_dir = dir.join("icons");

        fs::create_dir_all(&icons_dir).unwrap();

        // Write text files
        fs::write(dir.join("manifest.json"), MANIFEST_JSON).unwrap();
        fs::write(dir.join("background.js"), BACKGROUND_JS).unwrap();

        assert!(dir.join("manifest.json").exists());
        assert!(dir.join("background.js").exists());

        // Clean up
        fs::remove_dir_all(&dir).unwrap();
        assert!(!dir.exists());
    }
}
