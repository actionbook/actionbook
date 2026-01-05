/// Unit tests for URL generation and hash uniqueness
///
/// Tests verify that different handbook documents (action.md, overview.md)
/// generate unique URLs and hashes to prevent database conflicts.

use sha2::{Digest, Sha256};

/// Generate URL hash (same logic as documents::generate_url_hash)
fn generate_url_hash(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    hex::encode(hasher.finalize())
}

/// Simulate the URL generation logic from processor.rs
fn generate_document_url(base_url: &str, doc_name: &str) -> String {
    let handbook_type = doc_name
        .trim_end_matches(".md")
        .to_lowercase()
        .replace(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_', "-");
    format!("{}#handbook-{}", base_url.trim_end_matches('/'), handbook_type)
}

#[test]
fn test_current_implementation_prevents_hash_collision() {
    // Test the actual implementation logic
    let base_url = "https://dev.to";

    let action_url = generate_document_url(base_url, "action.md");
    let overview_url = generate_document_url(base_url, "overview.md");

    let action_hash = generate_url_hash(&action_url);
    let overview_hash = generate_url_hash(&overview_url);

    // Verify URLs are correctly formatted
    assert_eq!(action_url, "https://dev.to#handbook-action");
    assert_eq!(overview_url, "https://dev.to#handbook-overview");

    // Verify hashes are different (no collision)
    assert_ne!(
        action_hash, overview_hash,
        "action.md and overview.md must have different hashes"
    );
}

#[test]
fn test_fragment_identifiers_create_unique_hashes() {
    let base_url = "https://example.com";

    let action_url = format!("{}#handbook-action", base_url);
    let overview_url = format!("{}#handbook-overview", base_url);

    let action_hash = generate_url_hash(&action_url);
    let overview_hash = generate_url_hash(&overview_url);

    assert_ne!(
        action_hash, overview_hash,
        "Fragment identifiers must create unique hashes"
    );
}

#[test]
fn test_md_suffix_approach_creates_unique_hashes() {
    // Old approach: using .md suffix
    let base_url = "https://example.com";

    let action_url = format!("{}/action.md", base_url);
    let overview_url = format!("{}/overview.md", base_url);

    let action_hash = generate_url_hash(&action_url);
    let overview_hash = generate_url_hash(&overview_url);

    assert_ne!(
        action_hash, overview_hash,
        ".md suffix approach should create unique hashes"
    );
}

#[test]
fn test_same_base_url_creates_collision() {
    // Demonstrate the problem: using same URL for both documents
    let base_url = "https://example.com";

    let url1 = base_url.to_string();
    let url2 = base_url.to_string();

    let hash1 = generate_url_hash(&url1);
    let hash2 = generate_url_hash(&url2);

    // Same URL produces same hash (expected collision)
    assert_eq!(
        hash1, hash2,
        "Same URL must produce same hash (this is the problem we're avoiding)"
    );
}

#[test]
fn test_url_generation_with_trailing_slash() {
    // Test that trailing slashes are handled correctly
    let base_url_with_slash = "https://example.com/";
    let base_url_without_slash = "https://example.com";

    let url1 = generate_document_url(base_url_with_slash, "action.md");
    let url2 = generate_document_url(base_url_without_slash, "action.md");

    // Both should produce the same URL
    assert_eq!(url1, url2);
    assert_eq!(url1, "https://example.com#handbook-action");
}

#[test]
fn test_hash_consistency() {
    // Verify that the same URL always produces the same hash
    let url = "https://example.com#handbook-action";

    let hash1 = generate_url_hash(url);
    let hash2 = generate_url_hash(url);

    assert_eq!(hash1, hash2, "Hash function must be deterministic");
    assert_eq!(hash1.len(), 64, "SHA256 hash should be 64 characters");
}

#[test]
fn test_handbook_type_normalization() {
    // Test that doc_name is properly normalized
    let base_url = "https://example.com";

    // Test lowercase conversion
    let url1 = generate_document_url(base_url, "ACTION.md");
    assert_eq!(url1, "https://example.com#handbook-action");

    // Test space replacement
    let url2 = generate_document_url(base_url, "my action.md");
    assert_eq!(url2, "https://example.com#handbook-my-action");

    // Test special character replacement
    let url3 = generate_document_url(base_url, "action@docs.md");
    assert_eq!(url3, "https://example.com#handbook-action-docs");

    // Test mixed case and special chars
    let url4 = generate_document_url(base_url, "My_Action-File.md");
    assert_eq!(url4, "https://example.com#handbook-my_action-file");

    // Test multiple consecutive special chars
    let url5 = generate_document_url(base_url, "my  action!!.md");
    assert_eq!(url5, "https://example.com#handbook-my--action--");
}
