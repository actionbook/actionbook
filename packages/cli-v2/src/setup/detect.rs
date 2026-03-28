use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use colored::Colorize;

use crate::config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserInfo {
    pub name: String,
    pub path: PathBuf,
    pub version: Option<String>,
}

/// Detected environment information used to pre-fill setup defaults
#[derive(Debug)]
pub struct EnvironmentInfo {
    pub os: String,
    pub arch: String,
    pub shell: Option<String>,
    pub browsers: Vec<BrowserInfo>,
    pub npx_available: bool,
    pub node_version: Option<String>,
    pub existing_config: bool,
    pub existing_api_key: Option<String>,
}

/// Scan the system environment and return detected info
pub fn detect_environment() -> EnvironmentInfo {
    EnvironmentInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        shell: std::env::var("SHELL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        browsers: discover_all_browsers(),
        npx_available: which::which("npx").is_ok(),
        node_version: detect_node_version(),
        existing_config: config::config_path().exists(),
        existing_api_key: std::env::var("ACTIONBOOK_API_KEY").ok(),
    }
}

/// Print a formatted environment report to the terminal
pub fn print_environment_report(env: &EnvironmentInfo, json: bool) {
    if json {
        let browsers_json: Vec<serde_json::Value> = env
            .browsers
            .iter()
            .map(|b| {
                serde_json::json!({
                    "name": b.name,
                    "version": b.version,
                    "path": b.path.display().to_string(),
                })
            })
            .collect();

        let report = serde_json::json!({
            "os": env.os,
            "arch": env.arch,
            "shell": env.shell,
            "browsers": browsers_json,
            "npx_available": env.npx_available,
            "node": env.node_version,
            "existing_config": env.existing_config,
            "existing_api_key": env.existing_api_key.is_some(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
        return;
    }

    let bar = "│".dimmed();

    // System
    println!("  {}  {}", bar, "System".bold());
    println!(
        "  {}    {} OS: {} ({})",
        bar,
        check_mark(),
        env.os,
        env.arch
    );
    if let Some(ref shell) = env.shell {
        let shell_name = shell.rsplit('/').next().unwrap_or(shell);
        println!("  {}    {} Shell: {}", bar, check_mark(), shell_name);
    } else {
        println!(
            "  {}    {} Shell: {}",
            bar,
            empty_mark(),
            "not detected".dimmed()
        );
    }

    // Browsers
    println!("  {}", bar);
    println!("  {}  {}", bar, "Browsers".bold());
    if env.browsers.is_empty() {
        println!("  {}    {} {}", bar, empty_mark(), "none detected".dimmed());
    } else {
        for browser in &env.browsers {
            let version_str = browser
                .version
                .as_deref()
                .map(|v| format!(" v{}", v))
                .unwrap_or_default();
            println!(
                "  {}    {} {}{}",
                bar,
                check_mark(),
                browser.name,
                version_str
            );
        }
    }

    // Runtime
    println!("  {}", bar);
    println!("  {}  {}", bar, "Runtime".bold());
    if let Some(ref ver) = env.node_version {
        println!("  {}    {} Node.js: {}", bar, check_mark(), ver);
    } else {
        println!(
            "  {}    {} Node.js: {}",
            bar,
            empty_mark(),
            "not detected".dimmed()
        );
    }
    if env.npx_available {
        println!("  {}    {} npx", bar, check_mark());
    } else {
        println!(
            "  {}    {} npx: {}",
            bar,
            empty_mark(),
            "not available".dimmed()
        );
    }

    println!("  {}", bar);
}

fn check_mark() -> colored::ColoredString {
    "✓".green()
}

fn empty_mark() -> colored::ColoredString {
    "○".dimmed()
}

fn discover_all_browsers() -> Vec<BrowserInfo> {
    browser_candidates()
        .into_iter()
        .filter_map(|(name, paths)| {
            paths.into_iter().find_map(|path| {
                if path.exists() {
                    Some(BrowserInfo {
                        name: name.to_string(),
                        version: detect_version(&path),
                        path,
                    })
                } else {
                    None
                }
            })
        })
        .collect()
}

fn browser_candidates() -> Vec<(&'static str, Vec<PathBuf>)> {
    #[cfg(target_os = "macos")]
    {
        vec![
            (
                "Google Chrome",
                vec![
                    PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
                    PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
                ],
            ),
            (
                "Brave",
                vec![PathBuf::from(
                    "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
                )],
            ),
            (
                "Microsoft Edge",
                vec![PathBuf::from(
                    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
                )],
            ),
            (
                "Arc",
                vec![PathBuf::from("/Applications/Arc.app/Contents/MacOS/Arc")],
            ),
        ]
    }

    #[cfg(target_os = "linux")]
    {
        vec![
            (
                "Google Chrome",
                vec![
                    PathBuf::from("/usr/bin/google-chrome"),
                    PathBuf::from("/usr/bin/google-chrome-stable"),
                    PathBuf::from("/usr/bin/google-chrome-beta"),
                ],
            ),
            (
                "Brave",
                vec![
                    PathBuf::from("/usr/bin/brave-browser"),
                    PathBuf::from("/usr/bin/brave"),
                ],
            ),
            (
                "Microsoft Edge",
                vec![
                    PathBuf::from("/usr/bin/microsoft-edge"),
                    PathBuf::from("/usr/bin/microsoft-edge-stable"),
                ],
            ),
            (
                "Chromium",
                vec![
                    PathBuf::from("/usr/bin/chromium"),
                    PathBuf::from("/usr/bin/chromium-browser"),
                    PathBuf::from("/snap/bin/chromium"),
                ],
            ),
        ]
    }

    #[cfg(target_os = "windows")]
    {
        vec![
            (
                "Google Chrome",
                vec![
                    PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
                    PathBuf::from(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"),
                ],
            ),
            (
                "Brave",
                vec![
                    PathBuf::from(
                        r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
                    ),
                    PathBuf::from(
                        r"C:\Program Files (x86)\BraveSoftware\Brave-Browser\Application\brave.exe",
                    ),
                ],
            ),
            (
                "Microsoft Edge",
                vec![
                    PathBuf::from(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
                    PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
                ],
            ),
        ]
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        vec![]
    }
}

fn detect_node_version() -> Option<String> {
    which::which("node").ok()?;
    let output = Command::new("node").arg("--version").output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn detect_version(path: &Path) -> Option<String> {
    if path.to_string_lossy().contains("Arc.app") {
        return None;
    }

    let mut child = Command::new(path)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(3);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let output = child.wait_with_output().ok()?;
                let version = String::from_utf8_lossy(&output.stdout);
                let version = version.trim();
                if let Some(index) = version.rfind(' ') {
                    return Some(version[index + 1..].to_string());
                }
                return Some(version.to_string());
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_environment_returns_valid_struct() {
        let env = detect_environment();
        assert!(!env.os.is_empty());
        assert!(!env.arch.is_empty());
    }

    #[test]
    fn test_print_environment_report_does_not_panic() {
        let env = detect_environment();
        print_environment_report(&env, false);
    }

    #[test]
    fn test_print_environment_report_json_does_not_panic() {
        let env = detect_environment();
        print_environment_report(&env, true);
    }

    #[test]
    fn test_environment_info_os_is_known_platform() {
        let env = detect_environment();
        assert!(
            [
                "linux",
                "macos",
                "windows",
                "freebsd",
                "dragonfly",
                "netbsd",
                "openbsd",
                "solaris"
            ]
            .contains(&env.os.as_str()),
            "Unexpected OS: {}",
            env.os
        );
    }

    #[test]
    fn test_print_environment_report_with_no_browsers() {
        let env = EnvironmentInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            shell: None,
            browsers: vec![],
            npx_available: false,
            node_version: None,
            existing_config: false,
            existing_api_key: None,
        };
        print_environment_report(&env, false);
        print_environment_report(&env, true);
    }

    #[test]
    fn test_print_environment_report_with_browsers_and_shell() {
        let browser = BrowserInfo {
            name: "Google Chrome".to_string(),
            path: std::path::PathBuf::from("/usr/bin/google-chrome"),
            version: Some("131.0.0.0".to_string()),
        };
        let env = EnvironmentInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            shell: Some("/bin/zsh".to_string()),
            browsers: vec![browser],
            npx_available: true,
            node_version: Some("v20.0.0".to_string()),
            existing_config: true,
            existing_api_key: Some("test-key".to_string()),
        };
        print_environment_report(&env, false);
        print_environment_report(&env, true);
    }

    #[test]
    fn test_detect_node_version_runs_without_panic() {
        let _version = detect_node_version();
    }
}
