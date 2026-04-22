pub fn suggest(_input: &str, _candidates: &[&str]) -> Vec<String> {
    unimplemented!("not yet implemented")
}

#[cfg(test)]
mod tests {
    use super::suggest;

    #[test]
    fn suggest_orders_by_distance_then_alpha() {
        let suggestions = suggest("tabs", &["list-tabs", "tab", "click"]);
        assert_eq!(
            suggestions,
            vec!["tab".to_string(), "list-tabs".to_string()]
        );
    }

    #[test]
    fn suggest_excludes_exact_match_and_handles_empty() {
        assert!(
            suggest("tab", &["tab", "click"]).is_empty(),
            "exact match should not self-suggest"
        );
        assert!(
            suggest("browsr", &["click", "hover", "fill"]).is_empty(),
            "distance > 2 should produce an empty suggestion list"
        );
    }
}
