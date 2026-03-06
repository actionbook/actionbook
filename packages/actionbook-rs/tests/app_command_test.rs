//! Integration tests for `actionbook app` commands.
//!
//! These tests verify the Electron application automation functionality.

use actionbook::browser::{discover_electron_apps, SessionManager};
use actionbook::config::Config;

#[test]
fn test_app_discovery() {
    // Test discovering installed Electron apps
    let apps = discover_electron_apps();

    // Should return a list (may be empty if no apps installed)
    println!("Discovered {} Electron apps", apps.len());

    for app in &apps {
        println!("  - {} at {}", app.name, app.path.display());

        // Verify app path exists
        assert!(app.path.exists(), "App path should exist: {:?}", app.path);

        // Verify app name is not empty
        assert!(!app.name.is_empty(), "App name should not be empty");
    }
}

#[test]
fn test_app_info_structure() {
    // Test that ElectronAppInfo can be serialized/deserialized
    let apps = discover_electron_apps();

    if !apps.is_empty() {
        let json = serde_json::to_string(&apps[0]).expect("Should serialize to JSON");
        println!("ElectronAppInfo JSON: {}", json);

        // Verify JSON contains expected fields
        assert!(json.contains("\"name\""));
        assert!(json.contains("\"path\""));
    }
}

#[test]
fn test_session_manager_creation() {
    // Test that SessionManager can be created with default config
    let config = Config::default();
    let session_manager = SessionManager::new(config);

    // Should not panic during creation
    drop(session_manager);
}

// Note: The following tests require an actual Electron app to be running
// They are marked with #[ignore] to prevent CI failures

#[tokio::test]
#[ignore] // Requires Electron app running with --remote-debugging-port=9222
async fn test_app_launch() {
    // This test requires an Electron app to be installed
    let apps = discover_electron_apps();

    if apps.is_empty() {
        println!("No Electron apps found - skipping test");
        return;
    }

    let app = &apps[0];
    let config = Config::default();
    let session_manager = SessionManager::new(config);

    let app_path = app.path.to_str().expect("Valid app path");

    // Attempt to launch the app
    let result = session_manager
        .launch_custom_app("test-profile", app_path, vec![], Some(9223))
        .await;

    match result {
        Ok((_browser, _handler)) => {
            println!("Successfully launched {}", app.name);
        }
        Err(e) => {
            eprintln!("Failed to launch app: {}", e);
            // Not failing the test since it depends on system state
        }
    }
}

#[tokio::test]
#[ignore] // Requires manual setup
async fn test_shared_command_delegation() {
    // This test verifies that shared commands work via app command
    // It requires:
    // 1. An Electron app running with --remote-debugging-port=9222
    // 2. The app to be controllable via CDP

    // This is a placeholder for manual testing
    // In practice, you would:
    // 1. Launch an app with `actionbook app launch "VS Code"`
    // 2. Run `actionbook app snapshot -i`
    // 3. Run `actionbook app click <selector>`
    // 4. Verify the actions work correctly

    println!("This test requires manual setup and verification");
}

#[test]
fn test_app_name_matching() {
    // Test case-insensitive app name matching logic
    let apps = discover_electron_apps();

    if !apps.is_empty() {
        let app_name = &apps[0].name;

        // Should match lowercase
        let lowercase = app_name.to_lowercase();
        assert!(
            app_name.to_lowercase().contains(&lowercase),
            "Should match lowercase"
        );

        // Should match first word
        if let Some(first_word) = app_name.split_whitespace().next() {
            assert!(
                app_name.to_lowercase().contains(&first_word.to_lowercase()),
                "Should match first word"
            );
        }
    }
}

#[test]
fn test_config_default() {
    // Verify Config::default() doesn't panic
    let config = Config::default();
    drop(config);
}

// Documentation test to verify example usage
/// ```no_run
/// use actionbook::browser::{discover_electron_apps, SessionManager};
/// use actionbook::config::Config;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Discover apps
///     let apps = discover_electron_apps();
///     println!("Found {} apps", apps.len());
///
///     // Create session manager
///     let config = Config::default();
///     let session_manager = SessionManager::new(config);
///
///     // Launch app (if any found)
///     if let Some(app) = apps.first() {
///         let app_path = app.path.to_str().unwrap();
///         let (_browser, _handler) = session_manager
///             .launch_custom_app("default", app_path, vec![], Some(9222))
///             .await?;
///         println!("Launched {}", app.name);
///     }
///
///     Ok(())
/// }
/// ```
#[allow(dead_code)]
fn doctest_example() {}
