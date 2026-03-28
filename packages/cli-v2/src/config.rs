use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::browser::session::start::Cmd as StartCmd;
use crate::error::CliError;
use crate::types::Mode;

pub(crate) const DEFAULT_PROFILE: &str = "actionbook";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct ConfigFile {
    browser: BrowserConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct BrowserConfig {
    mode: Mode,
    #[serde(default = "default_profile_name")]
    default_profile: String,
    headless: bool,
    executable: Option<String>,
    #[serde(alias = "cdp-endpoint")]
    cdp_endpoint: Option<String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Local,
            default_profile: default_profile_name(),
            headless: false,
            executable: None,
            cdp_endpoint: None,
        }
    }
}

fn default_profile_name() -> String {
    DEFAULT_PROFILE.to_string()
}

pub fn actionbook_home() -> PathBuf {
    if let Ok(home) = std::env::var("ACTIONBOOK_HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".actionbook")
}

pub fn config_path() -> PathBuf {
    actionbook_home().join("config.toml")
}

pub fn profiles_dir() -> PathBuf {
    actionbook_home().join("profiles")
}

fn ensure_actionbook_home() -> Result<PathBuf, CliError> {
    let dir = actionbook_home();
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    Ok(dir)
}

fn bootstrap_default_config_if_missing() -> Result<PathBuf, CliError> {
    let path = config_path();
    if path.exists() {
        return Ok(path);
    }

    let dir = ensure_actionbook_home()?;
    let text = toml::to_string_pretty(&ConfigFile::default())
        .map_err(|e| CliError::Internal(format!("failed to serialize default config: {e}")))?;
    fs::write(&path, text)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }

    let _ = dir;
    Ok(path)
}

fn load_config() -> Result<ConfigFile, CliError> {
    let path = bootstrap_default_config_if_missing()?;
    let text = fs::read_to_string(&path)?;
    toml::from_str(&text).map_err(|e| {
        CliError::InvalidArgument(format!("invalid config file {}: {e}", path.display()))
    })
}

fn read_trimmed_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_env_bool(name: &str) -> Result<Option<bool>, CliError> {
    let Some(value) = read_trimmed_env(name) else {
        return Ok(None);
    };

    let normalized = value.to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "true" | "yes" | "on" => Ok(Some(true)),
        "0" | "false" | "no" | "off" => Ok(Some(false)),
        _ => Err(CliError::InvalidArgument(format!(
            "invalid boolean in {name}: {value}"
        ))),
    }
}

fn parse_env_mode(name: &str) -> Result<Option<Mode>, CliError> {
    let Some(value) = read_trimmed_env(name) else {
        return Ok(None);
    };

    Mode::from_str(&value)
        .map(Some)
        .map_err(|e| CliError::InvalidArgument(format!("{name}: {e}")))
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub fn resolve_start_command(mut cmd: StartCmd) -> Result<StartCmd, CliError> {
    let config = load_config()?;

    let env_mode = parse_env_mode("ACTIONBOOK_BROWSER_MODE")?;
    let env_profile = read_trimmed_env("ACTIONBOOK_BROWSER_DEFAULT_PROFILE");
    let env_headless = parse_env_bool("ACTIONBOOK_BROWSER_HEADLESS")?;
    let env_executable = read_trimmed_env("ACTIONBOOK_BROWSER_EXECUTABLE");
    let env_cdp = read_trimmed_env("ACTIONBOOK_BROWSER_CDP_ENDPOINT");

    let config_profile = normalize_optional(Some(config.browser.default_profile.clone()));
    let config_executable = normalize_optional(config.browser.executable.clone());
    let config_cdp = normalize_optional(config.browser.cdp_endpoint.clone());

    let resolved_mode = cmd.mode.or(env_mode).unwrap_or(config.browser.mode);
    let resolved_headless = cmd
        .headless
        .unwrap_or_else(|| env_headless.unwrap_or(config.browser.headless));

    let cli_profile = normalize_optional(cmd.profile.clone());
    let resolved_profile = cli_profile
        .clone()
        .or_else(|| env_profile.clone())
        .or_else(|| config_profile.clone())
        .unwrap_or_else(default_profile_name);
    let explicit_profile = cli_profile.is_some()
        || env_profile.is_some()
        || config_profile.as_deref() != Some(DEFAULT_PROFILE);

    cmd.mode = Some(resolved_mode);
    cmd.headless = Some(resolved_headless);
    cmd.profile = explicit_profile.then_some(resolved_profile);
    cmd.executable = env_executable.or(config_executable);
    cmd.cdp_endpoint = normalize_optional(cmd.cdp_endpoint)
        .or(env_cdp)
        .or(config_cdp);

    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

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

    fn make_home() -> (TempDir, EnvGuard) {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("actionbook-home");
        let guard = EnvGuard::set(&[
            ("ACTIONBOOK_HOME", Some(home.to_string_lossy().as_ref())),
            ("ACTIONBOOK_BROWSER_MODE", None),
            ("ACTIONBOOK_BROWSER_DEFAULT_PROFILE", None),
            ("ACTIONBOOK_BROWSER_HEADLESS", None),
            ("ACTIONBOOK_BROWSER_EXECUTABLE", None),
            ("ACTIONBOOK_BROWSER_CDP_ENDPOINT", None),
        ]);
        (tmp, guard)
    }

    fn base_cmd() -> StartCmd {
        StartCmd {
            mode: None,
            headless: None,
            profile: None,
            executable: None,
            open_url: None,
            cdp_endpoint: None,
            header: None,
            set_session_id: None,
        }
    }

    #[test]
    fn bootstrap_default_config_on_first_resolve() {
        let _lock = test_lock();
        let (_tmp, _guard) = make_home();

        let resolved = resolve_start_command(base_cmd()).expect("resolve");
        let path = config_path();
        let text = fs::read_to_string(&path).expect("read config");

        assert!(path.exists(), "config should be bootstrapped");
        assert!(text.contains("[browser]"));
        assert!(text.contains("default_profile = \"actionbook\""));
        assert_eq!(resolved.mode, Some(Mode::Local));
        assert_eq!(resolved.headless, Some(false));
        assert!(
            resolved.profile.is_none(),
            "default profile should stay implicit"
        );
    }

    #[test]
    fn env_overrides_config_for_all_phase1_fields() {
        let _lock = test_lock();
        let (_tmp, _guard) = make_home();
        fs::create_dir_all(actionbook_home()).expect("home");
        fs::write(
            config_path(),
            r#"[browser]
mode = "extension"
default_profile = "config-profile"
headless = false
executable = "/config/browser"
cdp_endpoint = "ws://127.0.0.1:9333/devtools/browser/config"
"#,
        )
        .expect("write config");

        let _env = EnvGuard::set(&[
            ("ACTIONBOOK_BROWSER_MODE", Some("cloud")),
            ("ACTIONBOOK_BROWSER_DEFAULT_PROFILE", Some("env-profile")),
            ("ACTIONBOOK_BROWSER_HEADLESS", Some("true")),
            ("ACTIONBOOK_BROWSER_EXECUTABLE", Some("/env/browser")),
            (
                "ACTIONBOOK_BROWSER_CDP_ENDPOINT",
                Some("ws://127.0.0.1:9444/devtools/browser/env"),
            ),
        ]);

        let resolved = resolve_start_command(base_cmd()).expect("resolve");

        assert_eq!(resolved.mode, Some(Mode::Cloud));
        assert_eq!(resolved.headless, Some(true));
        assert_eq!(resolved.profile.as_deref(), Some("env-profile"));
        assert_eq!(resolved.executable.as_deref(), Some("/env/browser"));
        assert_eq!(
            resolved.cdp_endpoint.as_deref(),
            Some("ws://127.0.0.1:9444/devtools/browser/env")
        );
    }

    #[test]
    fn cli_overrides_env_for_mode_profile_headless_and_cdp_endpoint() {
        let _lock = test_lock();
        let (_tmp, _guard) = make_home();
        let _env = EnvGuard::set(&[
            ("ACTIONBOOK_BROWSER_MODE", Some("extension")),
            ("ACTIONBOOK_BROWSER_DEFAULT_PROFILE", Some("env-profile")),
            ("ACTIONBOOK_BROWSER_HEADLESS", Some("false")),
            ("ACTIONBOOK_BROWSER_EXECUTABLE", Some("/env/browser")),
            (
                "ACTIONBOOK_BROWSER_CDP_ENDPOINT",
                Some("ws://127.0.0.1:9444/devtools/browser/env"),
            ),
        ]);

        let mut cmd = base_cmd();
        cmd.mode = Some(Mode::Local);
        cmd.headless = Some(true);
        cmd.profile = Some("cli-profile".to_string());
        cmd.cdp_endpoint = Some("ws://127.0.0.1:9555/devtools/browser/cli".to_string());

        let resolved = resolve_start_command(cmd).expect("resolve");

        assert_eq!(resolved.mode, Some(Mode::Local));
        assert_eq!(resolved.headless, Some(true));
        assert_eq!(resolved.profile.as_deref(), Some("cli-profile"));
        assert_eq!(
            resolved.cdp_endpoint.as_deref(),
            Some("ws://127.0.0.1:9555/devtools/browser/cli")
        );
    }

    #[test]
    fn cli_false_headless_overrides_env_true() {
        let _lock = test_lock();
        let (_tmp, _guard) = make_home();
        let _env = EnvGuard::set(&[("ACTIONBOOK_BROWSER_HEADLESS", Some("true"))]);

        let mut cmd = base_cmd();
        cmd.headless = Some(false);

        let resolved = resolve_start_command(cmd).expect("resolve");

        assert_eq!(resolved.headless, Some(false));
    }
}
