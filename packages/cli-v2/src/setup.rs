use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use clap::Args;

use crate::config::{self, ConfigFile};
use crate::error::CliError;
use crate::types::Mode;

#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct Cmd {
    /// Configuration target
    #[arg(long)]
    pub target: Option<String>,

    /// API key
    #[arg(long)]
    pub api_key: Option<String>,

    /// Browser configuration
    #[arg(long)]
    pub browser: Option<String>,

    /// Non-interactive mode
    #[arg(long)]
    pub non_interactive: bool,

    /// Reset configuration
    #[arg(long)]
    pub reset: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserInfo {
    pub name: String,
    pub path: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentInfo {
    pub os: String,
    pub arch: String,
    pub shell: Option<String>,
    pub browsers: Vec<BrowserInfo>,
    pub node_version: Option<String>,
    pub npx_available: bool,
}

pub fn execute(cmd: &Cmd, json: bool) -> Result<(), CliError> {
    let env = detect_environment();
    if !json {
        print_environment_report(&env);
    }

    if !should_save(cmd) {
        return Ok(());
    }

    let mut config = if cmd.reset {
        ConfigFile::default()
    } else {
        config::load_config()?
    };

    if let Some(api_key) = normalize_optional(cmd.api_key.clone()) {
        config.api.api_key = Some(api_key);
    }

    if let Some(browser) = normalize_optional(cmd.browser.clone()) {
        config.browser.mode = parse_browser_mode(&browser)?;
    }

    let path = config::save_config(&config)?;

    if !json {
        println!("setup: saved {}", path.display());
    }

    Ok(())
}

fn should_save(cmd: &Cmd) -> bool {
    if cmd.reset {
        return true;
    }

    cmd.non_interactive
        && (normalize_optional(cmd.api_key.clone()).is_some()
            || normalize_optional(cmd.browser.clone()).is_some())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_browser_mode(value: &str) -> Result<Mode, CliError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "isolated" | "local" => Ok(Mode::Local),
        "extension" => Ok(Mode::Extension),
        other => Err(CliError::InvalidArgument(format!(
            "invalid --browser value '{other}': expected isolated|local|extension"
        ))),
    }
}

pub fn detect_environment() -> EnvironmentInfo {
    EnvironmentInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        shell: std::env::var("SHELL")
            .ok()
            .map(|shell| shell.trim().to_string())
            .filter(|shell| !shell.is_empty()),
        browsers: discover_all_browsers(),
        node_version: detect_node_version(),
        npx_available: find_in_path("npx").is_some(),
    }
}

fn print_environment_report(env: &EnvironmentInfo) {
    println!("setup: environment");
    println!("  os: {} ({})", env.os, env.arch);

    if let Some(shell) = &env.shell {
        println!("  shell: {}", shell.rsplit('/').next().unwrap_or(shell));
    } else {
        println!("  shell: not detected");
    }

    if env.browsers.is_empty() {
        println!("  browsers: none detected");
    } else {
        println!("  browsers:");
        for browser in &env.browsers {
            let version = browser
                .version
                .as_deref()
                .map(|version| format!(" v{version}"))
                .unwrap_or_default();
            println!(
                "    - {}{} ({})",
                browser.name,
                version,
                browser.path.display()
            );
        }
    }

    if let Some(node_version) = &env.node_version {
        println!("  node: {node_version}");
    } else {
        println!("  node: not detected");
    }

    println!("  npx: {}", if env.npx_available { "yes" } else { "no" });
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
    let node = find_in_path("node")?;
    let output = Command::new(node).arg("--version").output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn find_in_path(binary: &str) -> Option<PathBuf> {
    which::which(binary).ok()
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
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().expect("lock")
    }

    struct EnvGuard {
        saved: Vec<(&'static str, Option<String>)>,
    }

    impl EnvGuard {
        fn set(pairs: &[(&'static str, Option<&str>)]) -> Self {
            let mut saved = Vec::new();
            for (key, value) in pairs {
                saved.push((*key, std::env::var(key).ok()));
                match value {
                    Some(value) => unsafe { std::env::set_var(key, value) },
                    None => unsafe { std::env::remove_var(key) },
                }
            }
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.saved.drain(..) {
                match value {
                    Some(value) => unsafe { std::env::set_var(key, value) },
                    None => unsafe { std::env::remove_var(key) },
                }
            }
        }
    }

    fn make_home() -> (tempfile::TempDir, EnvGuard) {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("actionbook-home");
        let guard = EnvGuard::set(&[("ACTIONBOOK_HOME", Some(home.to_string_lossy().as_ref()))]);
        (tmp, guard)
    }

    #[test]
    fn parse_browser_mode_accepts_setup_aliases() {
        assert_eq!(parse_browser_mode("isolated").unwrap(), Mode::Local);
        assert_eq!(parse_browser_mode("local").unwrap(), Mode::Local);
        assert_eq!(parse_browser_mode("extension").unwrap(), Mode::Extension);
    }

    #[test]
    fn parse_browser_mode_rejects_unknown_values() {
        let err = parse_browser_mode("cloud").expect_err("cloud should be rejected");
        assert_eq!(err.error_code(), "INVALID_ARGUMENT");
    }

    #[test]
    fn detect_environment_returns_os_and_arch() {
        let env = detect_environment();
        assert!(!env.os.is_empty());
        assert!(!env.arch.is_empty());
    }

    #[test]
    fn find_in_path_resolves_binaries_from_path() {
        let _lock = test_lock();
        let tmp = tempfile::tempdir().expect("tmpdir");
        let bin_dir = tmp.path().join("bin");
        std::fs::create_dir_all(&bin_dir).expect("create bin dir");

        #[cfg(windows)]
        let binary = bin_dir.join("fake-node.cmd");
        #[cfg(not(windows))]
        let binary = bin_dir.join("fake-node");

        std::fs::write(&binary, "@echo off\r\n").expect("write fake binary");

        #[cfg(unix)]
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake binary");

        let binary_name = binary
            .file_stem()
            .and_then(|name| name.to_str())
            .expect("binary name");

        let path = std::env::join_paths([bin_dir.clone()]).expect("join path");
        let _guard = EnvGuard::set(&[("PATH", Some(path.to_string_lossy().as_ref()))]);

        let resolved = find_in_path(binary_name).expect("binary should resolve from PATH");
        assert_eq!(resolved, binary);
    }

    #[test]
    fn execute_non_interactive_writes_api_key_and_browser_mode() {
        let _lock = test_lock();
        let (_tmp, _guard) = make_home();

        let cmd = Cmd {
            api_key: Some("sk-test".to_string()),
            browser: Some("isolated".to_string()),
            non_interactive: true,
            ..Cmd::default()
        };

        execute(&cmd, true).expect("execute setup");

        let config = config::load_config().expect("load config");
        assert_eq!(config.api.api_key.as_deref(), Some("sk-test"));
        assert_eq!(config.browser.mode, Mode::Local);
    }

    #[test]
    fn execute_requires_non_interactive_for_direct_config_writes() {
        let _lock = test_lock();
        let (_tmp, _guard) = make_home();

        let cmd = Cmd {
            api_key: Some("sk-test".to_string()),
            browser: Some("isolated".to_string()),
            ..Cmd::default()
        };

        execute(&cmd, true).expect("execute setup");

        assert!(
            !config::config_path().exists(),
            "setup should not persist config outside --non-interactive or --reset"
        );
    }

    #[test]
    fn execute_reset_recreates_default_config() {
        let _lock = test_lock();
        let (_tmp, _guard) = make_home();

        let initial = Cmd {
            api_key: Some("sk-test".to_string()),
            browser: Some("extension".to_string()),
            non_interactive: true,
            ..Cmd::default()
        };
        execute(&initial, true).expect("seed config");

        let reset = Cmd {
            reset: true,
            non_interactive: true,
            ..Cmd::default()
        };
        execute(&reset, true).expect("reset config");

        let config = config::load_config().expect("load reset config");
        assert_eq!(config.api.api_key, None);
        assert_eq!(config.browser.mode, Mode::Local);
        assert_eq!(
            config.browser.default_profile,
            crate::config::DEFAULT_PROFILE
        );
        assert!(config::config_path().exists(), "config file should exist");
    }

    #[test]
    fn execute_reset_without_non_interactive_still_clears_config() {
        let _lock = test_lock();
        let (_tmp, _guard) = make_home();

        let initial = Cmd {
            api_key: Some("sk-test".to_string()),
            browser: Some("extension".to_string()),
            non_interactive: true,
            ..Cmd::default()
        };
        execute(&initial, true).expect("seed config");

        let reset = Cmd {
            reset: true,
            ..Cmd::default()
        };
        execute(&reset, true).expect("reset config");

        let config = config::load_config().expect("load reset config");
        assert_eq!(config.api.api_key, None);
        assert_eq!(config.browser.mode, Mode::Local);
        assert_eq!(
            config.browser.default_profile,
            crate::config::DEFAULT_PROFILE
        );
        assert!(config::config_path().exists(), "config file should exist");
    }
}
