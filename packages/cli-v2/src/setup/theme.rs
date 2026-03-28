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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_theme_returns_colorful_theme() {
        let _theme = setup_theme();
    }

    #[test]
    fn setup_theme_can_be_called_multiple_times() {
        let _t1 = setup_theme();
        let _t2 = setup_theme();
    }
}
