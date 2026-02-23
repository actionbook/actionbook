//! Accessibility tree parsing and CSS selector matching

use super::types::AccessibilityNode;

/// Extension trait for accessibility tree operations
pub trait AccessibilityTreeExt {
    /// Find the element reference matching a CSS selector
    fn find_matching(&self, selector: &str) -> Option<&str>;

    /// Check if this node matches the given selector
    fn matches_selector(&self, selector: &str) -> bool;
}

impl AccessibilityTreeExt for AccessibilityNode {
    fn find_matching(&self, selector: &str) -> Option<&str> {
        // First check if current node matches
        if self.matches_selector(selector) {
            return self.element_ref.as_deref();
        }

        // Recursively search children
        if let Some(children) = &self.children {
            for child in children {
                if let Some(element_ref) = child.find_matching(selector) {
                    return Some(element_ref);
                }
            }
        }

        None
    }

    fn matches_selector(&self, selector: &str) -> bool {
        let selector = selector.trim();

        // Match by ID: #login-btn
        if let Some(id) = selector.strip_prefix('#') {
            if let Some(name) = &self.name {
                return name.contains(id) || name == id;
            }
            return false;
        }

        // Match by class: .btn-primary (match name contains)
        if let Some(class) = selector.strip_prefix('.') {
            if let Some(name) = &self.name {
                return name.contains(class);
            }
            return false;
        }

        // Match by tag name: button, input, a, etc.
        if !selector.contains('[') && !selector.contains(':') {
            return self.role.eq_ignore_ascii_case(selector);
        }

        // Match by attribute: [aria-label="Submit"], [type="submit"]
        if selector.starts_with('[') && selector.ends_with(']') {
            return self.matches_attribute_selector(selector);
        }

        // Match by text content: button:contains("Login")
        if let Some(text_start) = selector.find(":contains(") {
            let role = &selector[..text_start];
            if !role.is_empty() && !self.role.eq_ignore_ascii_case(role) {
                return false;
            }

            if let Some(text_end) = selector.rfind(')') {
                let text = &selector[text_start + 10..text_end]; // Skip ":contains("
                let text = text.trim_matches('"').trim_matches('\'');

                if let Some(name) = &self.name {
                    return name.contains(text);
                }
            }
        }

        false
    }
}

impl AccessibilityNode {
    /// Match attribute selectors like [aria-label="Submit"]
    fn matches_attribute_selector(&self, selector: &str) -> bool {
        let inner = &selector[1..selector.len() - 1]; // Remove [ and ]

        // Split by = to get attribute and value
        if let Some(eq_pos) = inner.find('=') {
            let attr = inner[..eq_pos].trim();
            let value = inner[eq_pos + 1..].trim().trim_matches('"').trim_matches('\'');

            match attr {
                "aria-label" | "name" => {
                    if let Some(name) = &self.name {
                        return name == value || name.contains(value);
                    }
                }
                "role" => return self.role == value,
                "type" => {
                    // For input elements, match role
                    if self.role == "textbox" && value == "text" {
                        return true;
                    }
                    if self.role == "button" && value == "submit" {
                        return true;
                    }
                }
                _ => {}
            }
        } else {
            // Just checking attribute exists
            let attr = inner.trim();
            match attr {
                "focusable" => return self.focusable.unwrap_or(false),
                _ => {}
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_node(role: &str, name: Option<&str>, element_ref: Option<&str>) -> AccessibilityNode {
        AccessibilityNode {
            role: role.to_string(),
            name: name.map(|s| s.to_string()),
            element_ref: element_ref.map(|s| s.to_string()),
            children: None,
            value: None,
            focusable: None,
        }
    }

    #[test]
    fn test_match_by_id() {
        let node = create_test_node("button", Some("login-btn"), Some("e1"));
        assert!(node.matches_selector("#login-btn"));
        assert!(!node.matches_selector("#signup-btn"));
    }

    #[test]
    fn test_match_by_class() {
        let node = create_test_node("button", Some("btn-primary"), Some("e1"));
        assert!(node.matches_selector(".btn-primary"));
        assert!(node.matches_selector(".primary")); // Partial match
        assert!(!node.matches_selector(".secondary"));
    }

    #[test]
    fn test_match_by_role() {
        let node = create_test_node("button", Some("Submit"), Some("e1"));
        assert!(node.matches_selector("button"));
        assert!(!node.matches_selector("textbox"));
    }

    #[test]
    fn test_match_by_text_content() {
        let node = create_test_node("button", Some("Login to Account"), Some("e1"));
        assert!(node.matches_selector("button:contains(\"Login\")"));
        assert!(node.matches_selector(":contains(\"Account\")")); // Any role
        assert!(!node.matches_selector("button:contains(\"Logout\")"));
    }

    #[test]
    fn test_match_by_attribute() {
        let node = create_test_node("button", Some("Submit"), Some("e1"));
        assert!(node.matches_selector("[aria-label=\"Submit\"]"));
        assert!(node.matches_selector("[name=\"Submit\"]"));
        assert!(node.matches_selector("[role=\"button\"]"));
    }

    #[test]
    fn test_find_matching_recursive() {
        let mut root = create_test_node("document", None, None);
        let child1 = create_test_node("div", None, None);
        let mut child2 = create_test_node("section", None, None);
        let target = create_test_node("button", Some("login-btn"), Some("e3"));

        child2.children = Some(vec![target]);
        root.children = Some(vec![child1, child2]);

        assert_eq!(root.find_matching("#login-btn"), Some("e3"));
        assert_eq!(root.find_matching("button"), Some("e3")); // Finds button in child2
    }

    #[test]
    fn test_find_matching_returns_first() {
        let button1 = create_test_node("button", Some("Submit"), Some("e1"));
        let button2 = create_test_node("button", Some("Submit"), Some("e2"));

        let mut root = create_test_node("document", None, None);
        root.children = Some(vec![button1, button2]);

        // Should return first matching element
        assert_eq!(root.find_matching("button"), Some("e1"));
    }
}
