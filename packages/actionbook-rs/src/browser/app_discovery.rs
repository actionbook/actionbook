//! Electron application discovery across platforms.
//!
//! Detects commonly installed Electron apps by searching platform-specific
//! application directories. Supports macOS, Linux, and Windows.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Information about a discovered Electron application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectronAppInfo {
    /// Application name (e.g., "Visual Studio Code")
    pub name: String,
    /// Full path to the executable
    pub path: PathBuf,
    /// Application version (if detectable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Well-known Electron applications with their platform-specific paths.
struct AppDefinition {
    name: &'static str,
    #[cfg(target_os = "macos")]
    macos_path: &'static str,
    #[cfg(target_os = "linux")]
    linux_paths: &'static [&'static str],
    #[cfg(target_os = "windows")]
    windows_paths: &'static [&'static str],
}

static KNOWN_APPS: &[AppDefinition] = &[
    AppDefinition {
        name: "Visual Studio Code",
        #[cfg(target_os = "macos")]
        macos_path: "/Applications/Visual Studio Code.app/Contents/MacOS/Electron",
        #[cfg(target_os = "linux")]
        linux_paths: &[
            "/usr/bin/code",
            "/usr/local/bin/code",
            "~/.local/bin/code",
        ],
        #[cfg(target_os = "windows")]
        windows_paths: &[
            r"C:\Program Files\Microsoft VS Code\Code.exe",
            r"C:\Program Files (x86)\Microsoft VS Code\Code.exe",
            r"%LOCALAPPDATA%\Programs\Microsoft VS Code\Code.exe",
        ],
    },
    AppDefinition {
        name: "Slack",
        #[cfg(target_os = "macos")]
        macos_path: "/Applications/Slack.app/Contents/MacOS/Slack",
        #[cfg(target_os = "linux")]
        linux_paths: &["/usr/bin/slack", "/snap/bin/slack"],
        #[cfg(target_os = "windows")]
        windows_paths: &[r"%LOCALAPPDATA%\slack\slack.exe"],
    },
    AppDefinition {
        name: "Discord",
        #[cfg(target_os = "macos")]
        macos_path: "/Applications/Discord.app/Contents/MacOS/Discord",
        #[cfg(target_os = "linux")]
        linux_paths: &["/usr/bin/discord", "/snap/bin/discord"],
        #[cfg(target_os = "windows")]
        windows_paths: &[r"%LOCALAPPDATA%\Discord\app-*\Discord.exe"],
    },
    AppDefinition {
        name: "Notion",
        #[cfg(target_os = "macos")]
        macos_path: "/Applications/Notion.app/Contents/MacOS/Notion",
        #[cfg(target_os = "linux")]
        linux_paths: &["/usr/bin/notion-app", "~/.local/bin/notion-app"],
        #[cfg(target_os = "windows")]
        windows_paths: &[r"%LOCALAPPDATA%\Programs\Notion\Notion.exe"],
    },
    AppDefinition {
        name: "Obsidian",
        #[cfg(target_os = "macos")]
        macos_path: "/Applications/Obsidian.app/Contents/MacOS/Obsidian",
        #[cfg(target_os = "linux")]
        linux_paths: &["/usr/bin/obsidian", "~/.local/bin/obsidian"],
        #[cfg(target_os = "windows")]
        windows_paths: &[r"%LOCALAPPDATA%\Obsidian\Obsidian.exe"],
    },
    AppDefinition {
        name: "Figma",
        #[cfg(target_os = "macos")]
        macos_path: "/Applications/Figma.app/Contents/MacOS/Figma",
        #[cfg(target_os = "linux")]
        linux_paths: &["/usr/bin/figma-linux"],
        #[cfg(target_os = "windows")]
        windows_paths: &[r"%LOCALAPPDATA%\Figma\Figma.exe"],
    },
    AppDefinition {
        name: "Spotify",
        #[cfg(target_os = "macos")]
        macos_path: "/Applications/Spotify.app/Contents/MacOS/Spotify",
        #[cfg(target_os = "linux")]
        linux_paths: &["/usr/bin/spotify", "/snap/bin/spotify"],
        #[cfg(target_os = "windows")]
        windows_paths: &[r"%APPDATA%\Spotify\Spotify.exe"],
    },
];

/// Discovers installed Electron applications on the current platform.
///
/// Returns a list of detected apps with their executable paths.
pub fn discover_electron_apps() -> Vec<ElectronAppInfo> {
    let mut apps = Vec::new();

    for app_def in KNOWN_APPS {
        if let Some(app_info) = detect_app(app_def) {
            apps.push(app_info);
        }
    }

    apps
}

#[cfg(target_os = "macos")]
fn detect_app(app_def: &AppDefinition) -> Option<ElectronAppInfo> {
    let path = PathBuf::from(app_def.macos_path);
    if path.exists() {
        Some(ElectronAppInfo {
            name: app_def.name.to_string(),
            path,
            version: None, // TODO: Parse version from Info.plist
        })
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn detect_app(app_def: &AppDefinition) -> Option<ElectronAppInfo> {
    for path_str in app_def.linux_paths {
        let expanded = shellexpand::tilde(path_str);
        let path = PathBuf::from(expanded.as_ref());
        if path.exists() {
            return Some(ElectronAppInfo {
                name: app_def.name.to_string(),
                path,
                version: None, // TODO: Detect version via --version
            });
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn detect_app(app_def: &AppDefinition) -> Option<ElectronAppInfo> {
    for path_str in app_def.windows_paths {
        // Expand environment variables
        let expanded = shellexpand::env(path_str).ok()?;
        let path = PathBuf::from(expanded.as_ref());

        // Handle glob patterns (e.g., app-*)
        if path_str.contains('*') {
            // Find the first path component that contains '*'
            // and read the parent directory before that component
            let parts: Vec<&str> = expanded.split(&['/', '\\'][..]).collect();
            let mut real_parent_parts = Vec::new();

            for part in &parts {
                if part.contains('*') {
                    break;
                }
                real_parent_parts.push(*part);
            }

            if !real_parent_parts.is_empty() {
                let real_parent = real_parent_parts.join(std::path::MAIN_SEPARATOR_STR);
                let real_parent_path = PathBuf::from(&real_parent);

                // Recursively search for matching executable
                if let Ok(entries) = std::fs::read_dir(real_parent_path) {
                    for entry in entries.flatten() {
                        let entry_path = entry.path();

                        // Try to match the full pattern by checking subdirectories
                        if entry_path.is_dir() {
                            // Look for the executable in subdirectories
                            if let Ok(sub_entries) = std::fs::read_dir(&entry_path) {
                                for sub_entry in sub_entries.flatten() {
                                    let sub_path = sub_entry.path();
                                    if sub_path.is_file()
                                        && sub_path
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .map(|n| n.to_lowercase() == app_def.name.to_lowercase() + ".exe")
                                            .unwrap_or(false)
                                    {
                                        return Some(ElectronAppInfo {
                                            name: app_def.name.to_string(),
                                            path: sub_path,
                                            version: None,
                                        });
                                    }
                                }
                            }
                        } else if entry_path.is_file()
                            && entry_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .map(|n| n.to_lowercase() == app_def.name.to_lowercase() + ".exe")
                                .unwrap_or(false)
                        {
                            return Some(ElectronAppInfo {
                                name: app_def.name.to_string(),
                                path: entry_path,
                                version: None,
                            });
                        }
                    }
                }
            }
        } else if path.exists() {
            return Some(ElectronAppInfo {
                name: app_def.name.to_string(),
                path,
                version: None,
            });
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn detect_app(_app_def: &AppDefinition) -> Option<ElectronAppInfo> {
    None
}
