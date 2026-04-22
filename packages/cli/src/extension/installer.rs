use std::path::{Path, PathBuf};

use serde_json::json;

use crate::action_result::ActionResult;
use crate::config;

pub const COMMAND_NAME_PATH: &str = "extension path";
pub const COMMAND_NAME_INSTALL: &str = "extension install";
pub const COMMAND_NAME_UNINSTALL: &str = "extension uninstall";

/// Extension files bundled at compile time from packages/actionbook-extension/.
///
/// Using include_bytes! ties the installed extension to the CLI version it was
/// built from — no network dependency, always consistent.
static BUNDLED_EXTENSION: &[(&str, &[u8])] = &[
    (
        "manifest.json",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/manifest.json"
        )),
    ),
    (
        "background.js",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/background.js"
        )),
    ),
    (
        "popup.html",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/popup.html"
        )),
    ),
    (
        "popup.js",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/popup.js"
        )),
    ),
    (
        "offscreen.html",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/offscreen.html"
        )),
    ),
    (
        "offscreen.js",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/offscreen.js"
        )),
    ),
    // Cloud Mode (extension v0.5.0+): PKCE OAuth sign-in flow. Omitting these
    // would silently produce a broken install for users of `actionbook extension
    // install` — popup.js would fail to load cloud-config.js and the OAuth
    // redirect would land on a missing callback.html.
    (
        "cloud-config.js",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/cloud-config.js"
        )),
    ),
    (
        "callback.html",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/callback.html"
        )),
    ),
    (
        "callback.js",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/callback.js"
        )),
    ),
    (
        "icons/icon-16.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/icons/icon-16.png"
        )),
    ),
    (
        "icons/icon-48.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/icons/icon-48.png"
        )),
    ),
    (
        "icons/icon-128.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../actionbook-extension/icons/icon-128.png"
        )),
    ),
];

fn legacy_extension_dir() -> PathBuf {
    config::actionbook_home().join("extension")
}

fn default_extension_dir_for_home(actionbook_home: &Path) -> PathBuf {
    match actionbook_home.file_name().and_then(|name| name.to_str()) {
        Some(".actionbook") => actionbook_home
            .parent()
            .unwrap_or(actionbook_home)
            .join("Actionbook")
            .join("extension"),
        _ => actionbook_home.join("extension"),
    }
}

fn preferred_extension_dir() -> PathBuf {
    default_extension_dir_for_home(&config::actionbook_home())
}

fn installed_extension_dir() -> Option<PathBuf> {
    let preferred = preferred_extension_dir();
    if preferred.join("manifest.json").exists() {
        return Some(preferred);
    }

    let legacy = legacy_extension_dir();
    if legacy != preferred && legacy.join("manifest.json").exists() {
        return Some(legacy);
    }

    None
}

fn extension_dir() -> PathBuf {
    installed_extension_dir().unwrap_or_else(preferred_extension_dir)
}

fn removable_extension_dirs() -> Vec<PathBuf> {
    let preferred = preferred_extension_dir();
    let legacy = legacy_extension_dir();
    if preferred == legacy {
        vec![preferred]
    } else {
        vec![preferred, legacy]
    }
}

fn remove_existing_extension_dirs() -> Result<(), std::io::Error> {
    for dir in removable_extension_dirs() {
        if let Err(e) = std::fs::remove_dir_all(&dir)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            return Err(e);
        }
    }
    Ok(())
}

fn read_version(dir: &Path) -> Option<String> {
    let bytes = std::fs::read(dir.join("manifest.json")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v["version"].as_str().map(String::from)
}

pub fn execute_path() -> ActionResult {
    let dir = extension_dir();
    let installed = dir.join("manifest.json").exists();
    let version = if installed { read_version(&dir) } else { None };
    ActionResult::ok(json!({
        "path": dir.to_string_lossy(),
        "installed": installed,
        "version": version,
        "required_version": crate::EXTENSION_PROTOCOL_MIN_VERSION,
    }))
}

pub fn execute_install(force: bool) -> ActionResult {
    let dir = preferred_extension_dir();

    if let Some(existing) = installed_extension_dir()
        && !force
    {
        return ActionResult::fatal(
            "ALREADY_INSTALLED",
            format!(
                "extension already installed at '{}'; use --force to overwrite",
                existing.display()
            ),
        );
    }

    // Test seam: copy from local source directory (used in e2e tests)
    if let Ok(src) = std::env::var("ACTIONBOOK_EXTENSION_TEST_SOURCE_DIR") {
        return copy_from_dir(Path::new(&src), &dir);
    }

    // Production: extract bundled extension files
    extract_bundled(&dir)
}

fn extract_bundled(dst: &Path) -> ActionResult {
    if let Err(e) = remove_existing_extension_dirs() {
        return ActionResult::fatal(
            "IO_ERROR",
            format!("failed to remove existing install: {e}"),
        );
    }
    if let Err(e) = std::fs::create_dir_all(dst) {
        return ActionResult::fatal(
            "IO_ERROR",
            format!("failed to create install directory: {e}"),
        );
    }

    for (relative_path, bytes) in BUNDLED_EXTENSION {
        let out = dst.join(relative_path);
        if let Some(parent) = out.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return ActionResult::fatal(
                "IO_ERROR",
                format!("failed to create directory '{}': {e}", parent.display()),
            );
        }
        if let Err(e) = std::fs::write(&out, bytes) {
            return ActionResult::fatal(
                "IO_ERROR",
                format!("failed to write '{}': {e}", out.display()),
            );
        }
    }

    let version = read_version(dst).unwrap_or_default();
    ActionResult::ok(json!({
        "path": dst.to_string_lossy(),
        "version": version,
        "required_version": crate::EXTENSION_PROTOCOL_MIN_VERSION,
    }))
}

fn copy_from_dir(src: &Path, dst: &Path) -> ActionResult {
    if let Err(e) = remove_existing_extension_dirs() {
        return ActionResult::fatal(
            "IO_ERROR",
            format!("failed to remove existing install: {e}"),
        );
    }
    if let Err(e) = std::fs::create_dir_all(dst) {
        return ActionResult::fatal(
            "IO_ERROR",
            format!("failed to create install directory: {e}"),
        );
    }
    if let Err(e) = copy_dir_all(src, dst) {
        return ActionResult::fatal("IO_ERROR", format!("failed to copy extension files: {e}"));
    }
    let version = read_version(dst).unwrap_or_default();
    ActionResult::ok(json!({
        "path": dst.to_string_lossy(),
        "version": version,
        "required_version": crate::EXTENSION_PROTOCOL_MIN_VERSION,
    }))
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if file_type.is_dir() {
            std::fs::create_dir_all(&dest)?;
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

pub fn execute_uninstall() -> ActionResult {
    let installed = removable_extension_dirs()
        .into_iter()
        .any(|dir| dir.join("manifest.json").exists());
    if !installed {
        return ActionResult::fatal("NOT_INSTALLED", "extension is not installed");
    }
    if let Err(e) = remove_existing_extension_dirs() {
        return ActionResult::fatal("IO_ERROR", format!("failed to remove extension: {e}"));
    }
    ActionResult::ok(json!({ "uninstalled": true }))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::default_extension_dir_for_home;

    #[test]
    fn visible_extension_install_dir_defaults_to_non_hidden_home_sibling() {
        let home = Path::new("/Users/test/.actionbook");
        let dir = default_extension_dir_for_home(home);

        assert_eq!(dir, Path::new("/Users/test/Actionbook/extension"));
    }

    #[test]
    fn custom_actionbook_home_keeps_extension_inside_custom_tree() {
        let home = Path::new("/tmp/actionbook-home");
        let dir = default_extension_dir_for_home(home);

        assert_eq!(dir, Path::new("/tmp/actionbook-home/extension"));
    }
}
