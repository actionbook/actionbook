use console::style;
use dialoguer::theme::ColorfulTheme;

/// Create the setup wizard theme with indented radio-button style options.
///
/// All prompts use a unified ◉/○ visual for both Select and MultiSelect:
/// ```text
/// Select browser ›
///   ◉ Chrome (detected)
///   ○ Built-in
/// ```
pub fn setup_theme() -> ColorfulTheme {
    ColorfulTheme {
        prompt_prefix: style("".to_string()).for_stderr(),
        active_item_prefix: style("  ◉ ".to_string()).for_stderr().green(),
        inactive_item_prefix: style("  ○ ".to_string()).for_stderr(),
        checked_item_prefix: style("  ◉ ".to_string()).for_stderr().green(),
        unchecked_item_prefix: style("  ○ ".to_string()).for_stderr(),
        ..ColorfulTheme::default()
    }
}
