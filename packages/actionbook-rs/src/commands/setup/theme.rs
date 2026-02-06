use console::style;
use dialoguer::theme::ColorfulTheme;

/// Create the setup wizard theme with indented option items.
///
/// Items are indented relative to the prompt to create visual hierarchy:
/// ```text
///   Select browser
///       > Chrome (detected)
///         Built-in
/// ```
pub fn setup_theme() -> ColorfulTheme {
    ColorfulTheme {
        prompt_prefix: style("".to_string()).for_stderr(),
        active_item_prefix: style("  › ".to_string()).for_stderr().green(),
        inactive_item_prefix: style("    ".to_string()).for_stderr(),
        checked_item_prefix: style("  ◉ ".to_string()).for_stderr().green(),
        unchecked_item_prefix: style("  ○ ".to_string()).for_stderr(),
        ..ColorfulTheme::default()
    }
}
