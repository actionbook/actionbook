use std::path::PathBuf;
use std::process::Command;

use crate::error::{ActionbookError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserType {
    Chrome,
    Brave,
    Edge,
    Arc,
    Chromium,
}

impl BrowserType {
    pub fn name(&self) -> &'static str {
        match self {
            BrowserType::Chrome => "Google Chrome",
            BrowserType::Brave => "Brave",
            BrowserType::Edge => "Microsoft Edge",
            BrowserType::Arc => "Arc",
            BrowserType::Chromium => "Chromium",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BrowserInfo {
    pub browser_type: BrowserType,
    pub path: PathBuf,
    pub version: Option<String>,
}

impl BrowserInfo {
    pub fn new(browser_type: BrowserType, path: PathBuf) -> Self {
        Self {
            browser_type,
            path,
            version: None,
        }
    }

    pub fn with_version(mut self) -> Self {
        self.version = detect_version(&self.path);
        self
    }
}

/// Discover the best available browser on the system
pub fn discover_browser() -> Result<BrowserInfo> {
    let browsers = discover_all_browsers();

    if browsers.is_empty() {
        return Err(ActionbookError::BrowserNotFound(
            "No supported browsers found. Please install Chrome, Brave, or Edge.".to_string(),
        ));
    }

    // Return the first (highest priority) browser
    Ok(browsers.into_iter().next().unwrap())
}

/// Discover all available browsers on the system
pub fn discover_all_browsers() -> Vec<BrowserInfo> {
    let candidates = get_browser_candidates();
    let mut found = Vec::new();

    for (browser_type, paths) in candidates {
        for path in paths {
            let path = PathBuf::from(path);
            if path.exists() {
                found.push(BrowserInfo::new(browser_type, path).with_version());
                break; // Found this browser type, move to next
            }
        }
    }

    found
}

/// Get browser candidates based on the current platform
fn get_browser_candidates() -> Vec<(BrowserType, Vec<&'static str>)> {
    #[cfg(target_os = "macos")]
    {
        vec![
            (
                BrowserType::Chrome,
                vec![
                    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                    "~/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                ],
            ),
            (
                BrowserType::Brave,
                vec![
                    "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
                    "~/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
                ],
            ),
            (
                BrowserType::Edge,
                vec![
                    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
                    "~/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
                ],
            ),
            (
                BrowserType::Arc,
                vec![
                    "/Applications/Arc.app/Contents/MacOS/Arc",
                    "~/Applications/Arc.app/Contents/MacOS/Arc",
                ],
            ),
            (
                BrowserType::Chromium,
                vec![
                    "/Applications/Chromium.app/Contents/MacOS/Chromium",
                    "~/Applications/Chromium.app/Contents/MacOS/Chromium",
                ],
            ),
        ]
    }

    #[cfg(target_os = "linux")]
    {
        vec![
            (
                BrowserType::Chrome,
                vec![
                    "/usr/bin/google-chrome",
                    "/usr/bin/google-chrome-stable",
                    "/usr/bin/google-chrome-beta",
                    "/snap/bin/chromium",
                ],
            ),
            (
                BrowserType::Brave,
                vec!["/usr/bin/brave-browser", "/usr/bin/brave"],
            ),
            (
                BrowserType::Edge,
                vec!["/usr/bin/microsoft-edge", "/usr/bin/microsoft-edge-stable"],
            ),
            (
                BrowserType::Chromium,
                vec!["/usr/bin/chromium", "/usr/bin/chromium-browser"],
            ),
        ]
    }

    #[cfg(target_os = "windows")]
    {
        vec![
            (
                BrowserType::Chrome,
                vec![
                    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
                    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
                ],
            ),
            (
                BrowserType::Brave,
                vec![
                    r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
                    r"C:\Program Files (x86)\BraveSoftware\Brave-Browser\Application\brave.exe",
                ],
            ),
            (
                BrowserType::Edge,
                vec![
                    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ],
            ),
        ]
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        vec![]
    }
}

/// Detect browser version
fn detect_version(path: &PathBuf) -> Option<String> {
    let output = Command::new(path).arg("--version").output().ok()?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout);
        // Extract version number from output like "Google Chrome 120.0.6099.109"
        let version = version.trim();
        // Try to extract just the version number
        if let Some(idx) = version.rfind(' ') {
            return Some(version[idx + 1..].to_string());
        }
        Some(version.to_string())
    } else {
        None
    }
}

/// Try to find browser using `which` command (Unix) or `where` (Windows)
#[allow(dead_code)]
pub fn find_browser_in_path(name: &str) -> Option<PathBuf> {
    which::which(name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_browser() {
        // This test will pass on machines with a browser installed
        let result = discover_browser();
        if result.is_ok() {
            let info = result.unwrap();
            println!(
                "Found browser: {} at {:?} (version: {:?})",
                info.browser_type.name(),
                info.path,
                info.version
            );
        }
    }

    #[test]
    fn test_discover_all_browsers() {
        let browsers = discover_all_browsers();
        for browser in browsers {
            println!(
                "Found: {} at {:?} (version: {:?})",
                browser.browser_type.name(),
                browser.path,
                browser.version
            );
        }
    }
}
