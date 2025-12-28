//! Documents database operations

use crate::db::models::{Document, NewDocument};
use crate::db::DbPool;
use crate::error::Result;
use sha2::{Digest, Sha256};

/// Insert a new document
pub async fn insert_document(pool: &DbPool, doc: &NewDocument) -> Result<i32> {
    let row = sqlx::query_scalar::<_, i32>(
        r#"
        INSERT INTO documents (
            source_id, source_version_id, url, url_hash, title, description,
            content_md, content_hash, depth, status, version, breadcrumb
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'active', 1, '[]')
        RETURNING id
        "#,
    )
    .bind(doc.source_id)
    .bind(doc.source_version_id)
    .bind(&doc.url)
    .bind(&doc.url_hash)
    .bind(&doc.title)
    .bind(&doc.description)
    .bind(&doc.content_md)
    .bind(&doc.content_hash)
    .bind(doc.depth)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Get document by ID
pub async fn get_document_by_id(pool: &DbPool, document_id: i32) -> Result<Option<Document>> {
    let doc = sqlx::query_as::<_, Document>("SELECT * FROM documents WHERE id = $1")
        .bind(document_id)
        .fetch_optional(pool)
        .await?;

    Ok(doc)
}

/// Get document by source_id and url_hash
pub async fn get_document_by_url_hash(
    pool: &DbPool,
    source_id: i32,
    url_hash: &str,
) -> Result<Option<Document>> {
    let doc = sqlx::query_as::<_, Document>(
        "SELECT * FROM documents WHERE source_id = $1 AND url_hash = $2",
    )
    .bind(source_id)
    .bind(url_hash)
    .fetch_optional(pool)
    .await?;

    Ok(doc)
}

/// Update document content
pub async fn update_document_content(
    pool: &DbPool,
    document_id: i32,
    content_md: &str,
    content_hash: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE documents
        SET content_md = $2,
            content_hash = $3,
            version = version + 1,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(document_id)
    .bind(content_md)
    .bind(content_hash)
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete documents by source_id
pub async fn delete_documents_by_source(pool: &DbPool, source_id: i32) -> Result<u64> {
    let result = sqlx::query("DELETE FROM documents WHERE source_id = $1")
        .bind(source_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Generate URL hash (SHA256, full 64 chars)
pub fn generate_url_hash(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate content hash (SHA256, first 16 chars for brevity)
pub fn generate_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_url_hash() {
        let hash = generate_url_hash("https://example.com/page");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_generate_content_hash() {
        let hash = generate_content_hash("Hello, World!");
        assert_eq!(hash.len(), 16);
    }

    #[test]
    fn test_hash_consistency() {
        let url = "https://example.com";
        let hash1 = generate_url_hash(url);
        let hash2 = generate_url_hash(url);
        assert_eq!(hash1, hash2);
    }
}
