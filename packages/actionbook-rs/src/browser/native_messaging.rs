//! Chrome Native Messaging host for automatic token exchange.
//!
//! When Chrome invokes the actionbook binary as a native messaging host,
//! this module handles the stdin/stdout protocol to provide the bridge
//! session token to the extension without manual copy-paste.
//!
//! Protocol: each message is prefixed with a 4-byte little-endian uint32 length,
//! followed by UTF-8 JSON of that length.

use std::io::{self, Read, Write};

use crate::browser::extension_bridge;

/// Native messaging host name registered with Chrome.
pub const NATIVE_HOST_NAME: &str = "com.actionbook.bridge";

/// The stable extension ID derived from the public key in manifest.json.
pub const EXTENSION_ID: &str = "dpfioflkmnkklgjldmaggkodhlidkdcd";

/// Default bridge port (must match extension's BRIDGE_URL and CLI default).
const DEFAULT_BRIDGE_PORT: u16 = 19222;

/// Read one native messaging message from stdin.
fn read_message() -> io::Result<serde_json::Value> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    // Read 4-byte little-endian length prefix
    let mut len_bytes = [0u8; 4];
    handle.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    // Sanity check: Chrome caps at 1MB
    if len > 1_048_576 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Message too large",
        ));
    }

    // Read JSON payload
    let mut buf = vec![0u8; len];
    handle.read_exact(&mut buf)?;

    serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Write one native messaging message to stdout.
fn write_message(msg: &serde_json::Value) -> io::Result<()> {
    let payload = serde_json::to_vec(msg)?;
    let len = payload.len() as u32;

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(&len.to_le_bytes())?;
    handle.write_all(&payload)?;
    handle.flush()
}

/// Run as a Chrome Native Messaging host.
///
/// Called when the binary is invoked with a `chrome-extension://` argument
/// (Chrome's native messaging invocation pattern).
///
/// Reads one request from stdin, processes it, writes one response to stdout, then exits.
pub async fn run() -> crate::error::Result<()> {
    let msg = read_message().map_err(|e| {
        crate::error::ActionbookError::Other(format!("Failed to read native message: {}", e))
    })?;

    let msg_type = msg
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let response = match msg_type {
        "get_token" => {
            let token = extension_bridge::read_token_file().await;
            let bridge_running =
                extension_bridge::is_bridge_running(DEFAULT_BRIDGE_PORT).await;

            match token {
                Some(t) => serde_json::json!({
                    "type": "token",
                    "token": t,
                    "port": DEFAULT_BRIDGE_PORT,
                    "bridge_running": bridge_running,
                }),
                None => serde_json::json!({
                    "type": "error",
                    "error": "no_token",
                    "message": "Bridge is not running. Start with: actionbook extension serve",
                }),
            }
        }
        _ => serde_json::json!({
            "type": "error",
            "error": "unknown_type",
            "message": format!("Unknown message type: {}", msg_type),
        }),
    };

    write_message(&response).map_err(|e| {
        crate::error::ActionbookError::Other(format!("Failed to write native message: {}", e))
    })?;

    Ok(())
}

/// Platform-specific path for the native messaging host manifest.
pub fn native_host_manifest_path() -> crate::error::Result<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or_else(|| {
            crate::error::ActionbookError::Other("Cannot determine home directory".to_string())
        })?;
        Ok(home
            .join("Library/Application Support/Google/Chrome/NativeMessagingHosts")
            .join(format!("{}.json", NATIVE_HOST_NAME)))
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().ok_or_else(|| {
            crate::error::ActionbookError::Other("Cannot determine home directory".to_string())
        })?;
        Ok(home
            .join(".config/google-chrome/NativeMessagingHosts")
            .join(format!("{}.json", NATIVE_HOST_NAME)))
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, native messaging hosts are registered via the registry,
        // but the manifest file can be placed anywhere. We'll use AppData.
        let app_data = dirs::data_local_dir().ok_or_else(|| {
            crate::error::ActionbookError::Other("Cannot determine AppData directory".to_string())
        })?;
        Ok(app_data
            .join("Actionbook")
            .join(format!("{}.json", NATIVE_HOST_NAME)))
    }
}

/// Generate the native messaging host manifest JSON content.
/// The `binary_path` should be the absolute path to the actionbook binary.
pub fn generate_manifest(binary_path: &str) -> serde_json::Value {
    serde_json::json!({
        "name": NATIVE_HOST_NAME,
        "description": "Actionbook CLI - provides bridge session tokens to the browser extension",
        "path": binary_path,
        "type": "stdio",
        "allowed_origins": [
            format!("chrome-extension://{}/", EXTENSION_ID)
        ]
    })
}

/// Install the native messaging host manifest to the platform-specific location.
pub fn install_manifest() -> crate::error::Result<std::path::PathBuf> {
    let manifest_path = native_host_manifest_path()?;

    // Determine actionbook binary path
    let binary_path = std::env::current_exe()
        .map_err(|e| {
            crate::error::ActionbookError::Other(format!(
                "Cannot determine binary path: {}",
                e
            ))
        })?
        .to_string_lossy()
        .to_string();

    // Also check if the binary is available via `which` (prefer PATH-resolved path
    // for npm-installed binaries where current_exe may point to a temp location)
    let resolved_path = which::which("actionbook")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(binary_path);

    let manifest = generate_manifest(&resolved_path);

    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            crate::error::ActionbookError::Other(format!(
                "Failed to create directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    let content = serde_json::to_string_pretty(&manifest).map_err(|e| {
        crate::error::ActionbookError::Other(format!("Failed to serialize manifest: {}", e))
    })?;

    std::fs::write(&manifest_path, content).map_err(|e| {
        crate::error::ActionbookError::Other(format!(
            "Failed to write native messaging host manifest to {}: {}",
            manifest_path.display(),
            e
        ))
    })?;

    Ok(manifest_path)
}

/// Remove the native messaging host manifest.
pub fn uninstall_manifest() -> crate::error::Result<()> {
    let manifest_path = native_host_manifest_path()?;
    if manifest_path.exists() {
        std::fs::remove_file(&manifest_path).map_err(|e| {
            crate::error::ActionbookError::Other(format!(
                "Failed to remove native messaging host manifest: {}",
                e
            ))
        })?;
    }
    Ok(())
}

/// Check if the native messaging host manifest is installed.
pub fn is_manifest_installed() -> bool {
    native_host_manifest_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_manifest_structure() {
        let manifest = generate_manifest("/usr/local/bin/actionbook");
        assert_eq!(manifest["name"], NATIVE_HOST_NAME);
        assert_eq!(manifest["type"], "stdio");
        assert_eq!(manifest["path"], "/usr/local/bin/actionbook");

        let origins = manifest["allowed_origins"].as_array().unwrap();
        assert_eq!(origins.len(), 1);
        assert!(origins[0].as_str().unwrap().contains(EXTENSION_ID));
    }

    #[test]
    fn test_extension_id_format() {
        // Extension IDs are 32 lowercase characters a-p
        assert_eq!(EXTENSION_ID.len(), 32);
        assert!(EXTENSION_ID.chars().all(|c| c >= 'a' && c <= 'p'));
    }

    #[test]
    fn test_native_host_name_format() {
        // Chrome requires reverse-DNS format
        assert!(NATIVE_HOST_NAME.contains('.'));
        assert!(NATIVE_HOST_NAME
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '_'));
    }
}
