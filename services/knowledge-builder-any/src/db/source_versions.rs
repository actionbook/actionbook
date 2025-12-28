//! Source versions database operations

use crate::db::models::{NewSourceVersion, SourceVersion, SourceVersionStatus};
use crate::db::DbPool;
use crate::error::Result;

/// Create a new source version
///
/// Automatically increments version_number based on existing versions
pub async fn create_version(pool: &DbPool, new_version: &NewSourceVersion) -> Result<SourceVersion> {
    // Get the next version number
    let next_version: i32 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(version_number), 0) + 1
        FROM source_versions
        WHERE source_id = $1
        "#,
    )
    .bind(new_version.source_id)
    .fetch_one(pool)
    .await?;

    let version = sqlx::query_as::<_, SourceVersion>(
        r#"
        INSERT INTO source_versions (source_id, version_number, status, commit_message, created_by)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(new_version.source_id)
    .bind(next_version)
    .bind(SourceVersionStatus::Building.as_str())
    .bind(&new_version.commit_message)
    .bind(&new_version.created_by)
    .fetch_one(pool)
    .await?;

    Ok(version)
}

/// Get the latest active version for a source
pub async fn get_active_version(pool: &DbPool, source_id: i32) -> Result<Option<SourceVersion>> {
    let version = sqlx::query_as::<_, SourceVersion>(
        r#"
        SELECT * FROM source_versions
        WHERE source_id = $1 AND status = $2
        ORDER BY version_number DESC
        LIMIT 1
        "#,
    )
    .bind(source_id)
    .bind(SourceVersionStatus::Active.as_str())
    .fetch_optional(pool)
    .await?;

    Ok(version)
}

/// Publish a version (set status to active)
pub async fn publish_version(pool: &DbPool, version_id: i32) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE source_versions
        SET status = $1, published_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(SourceVersionStatus::Active.as_str())
    .bind(version_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Archive a version
pub async fn archive_version(pool: &DbPool, version_id: i32) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE source_versions
        SET status = $1
        WHERE id = $2
        "#,
    )
    .bind(SourceVersionStatus::Archived.as_str())
    .bind(version_id)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // Tests require a running database - see integration tests
}
