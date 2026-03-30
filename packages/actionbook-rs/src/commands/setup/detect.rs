use std::process::Command;

use colored::Colorize;

use crate::browser::{discover_all_browsers, BrowserInfo};
use crate::config::Config;

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
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let shell = std::env::var("SHELL").ok();
    let browsers = discover_all_browsers();
    let npx_available = which::which("npx").is_ok();
    let node_version = detect_node_version();
    let existing_config = Config::config_path().exists();
    let existing_api_key = std::env::var("ACTIONBOOK_API_KEY").ok();

    EnvironmentInfo {
        os,
        arch,
        shell,
        browsers,
        npx_available,
        node_version,
        existing_config,
        existing_api_key,
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
                    "name": b.browser_type.name(),
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
                browser.browser_type.name(),
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

fn detect_node_version() -> Option<String> {
    which::which("node").ok()?;
    let output = Command::new("node").arg("--version").output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::BrowserInfo;

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
        // std::env::consts::OS always returns a known string
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
        // Both modes should not panic with empty browsers
        print_environment_report(&env, false);
        print_environment_report(&env, true);
    }

    #[test]
    fn test_print_environment_report_with_browsers_and_shell() {
        let browser = BrowserInfo {
            browser_type: crate::browser::BrowserType::Chrome,
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
    fn test_print_environment_report_with_browser_no_version() {
        let browser = BrowserInfo {
            browser_type: crate::browser::BrowserType::Chrome,
            path: std::path::PathBuf::from("/usr/bin/google-chrome"),
            version: None,
        };
        let env = EnvironmentInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            shell: None,
            browsers: vec![browser],
            npx_available: false,
            node_version: None,
            existing_config: false,
            existing_api_key: None,
        };
        print_environment_report(&env, false);
    }

    #[test]
    fn test_detect_node_version_runs_without_panic() {
        // We don't assert the result since node may or may not be installed
        let _version = detect_node_version();
    }
}
