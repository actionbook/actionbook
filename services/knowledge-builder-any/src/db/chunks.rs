//! Chunks database operations

use crate::db::models::NewChunk;
use crate::db::DbPool;
use crate::error::Result;

/// Insert chunks with optional vector embeddings
///
/// Uses raw SQL for pgvector type casting
pub async fn insert_chunks(pool: &DbPool, chunks: &[NewChunk]) -> Result<()> {
    for chunk in chunks {
        let heading_hierarchy_json = serde_json::to_string(&chunk.heading_hierarchy)?;

        if let Some(embedding) = &chunk.embedding {
            // Format embedding as PostgreSQL array string for vector type
            let embedding_str = format!(
                "[{}]",
                embedding
                    .iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );

            sqlx::query(
                r#"
                INSERT INTO chunks (
                    document_id, source_version_id, content, content_hash, chunk_index,
                    start_char, end_char, heading, heading_hierarchy,
                    token_count, embedding, embedding_model
                ) VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb, $10, $11::vector, $12
                )
                "#,
            )
            .bind(chunk.document_id)
            .bind(chunk.source_version_id)
            .bind(&chunk.content)
            .bind(&chunk.content_hash)
            .bind(chunk.chunk_index)
            .bind(chunk.start_char)
            .bind(chunk.end_char)
            .bind(&chunk.heading)
            .bind(&heading_hierarchy_json)
            .bind(chunk.token_count)
            .bind(&embedding_str)
            .bind(&chunk.embedding_model)
            .execute(pool)
            .await?;
        } else {
            // Insert without embedding
            sqlx::query(
                r#"
                INSERT INTO chunks (
                    document_id, source_version_id, content, content_hash, chunk_index,
                    start_char, end_char, heading, heading_hierarchy,
                    token_count, embedding_model
                ) VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb, $10, $11
                )
                "#,
            )
            .bind(chunk.document_id)
            .bind(chunk.source_version_id)
            .bind(&chunk.content)
            .bind(&chunk.content_hash)
            .bind(chunk.chunk_index)
            .bind(chunk.start_char)
            .bind(chunk.end_char)
            .bind(&chunk.heading)
            .bind(&heading_hierarchy_json)
            .bind(chunk.token_count)
            .bind(&chunk.embedding_model)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

/// Delete all chunks for a document
pub async fn delete_chunks_by_document(pool: &DbPool, document_id: i32) -> Result<u64> {
    let result = sqlx::query("DELETE FROM chunks WHERE document_id = $1")
        .bind(document_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Count chunks for a document
pub async fn count_chunks_by_document(pool: &DbPool, document_id: i32) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chunks WHERE document_id = $1")
        .bind(document_id)
        .fetch_one(pool)
        .await?;

    Ok(row.0)
}

#[cfg(test)]
mod tests {
    // Tests require a running database - see integration tests
}
