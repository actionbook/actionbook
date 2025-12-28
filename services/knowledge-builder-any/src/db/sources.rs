//! Sources database operations

use crate::db::models::{NewSource, Source};
use crate::db::DbPool;
use crate::error::Result;

/// Create a new source record
pub async fn create_source(pool: &DbPool, source: &NewSource) -> Result<i32> {
    let row = sqlx::query_scalar::<_, i32>(
        r#"
        INSERT INTO sources (name, base_url, description, domain, crawl_config, tags)
        VALUES ($1, $2, $3, $4, '{}', '[]')
        RETURNING id
        "#,
    )
    .bind(&source.name)
    .bind(&source.base_url)
    .bind(&source.description)
    .bind(&source.domain)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Get source by base URL
pub async fn get_source_by_url(pool: &DbPool, base_url: &str) -> Result<Option<Source>> {
    let source = sqlx::query_as::<_, Source>("SELECT * FROM sources WHERE base_url = $1")
        .bind(base_url)
        .fetch_optional(pool)
        .await?;

    Ok(source)
}

/// Get source by ID
pub async fn get_source_by_id(pool: &DbPool, source_id: i32) -> Result<Option<Source>> {
    let source = sqlx::query_as::<_, Source>("SELECT * FROM sources WHERE id = $1")
        .bind(source_id)
        .fetch_optional(pool)
        .await?;

    Ok(source)
}

/// Get source by name
pub async fn get_source_by_name(pool: &DbPool, name: &str) -> Result<Option<Source>> {
    let source = sqlx::query_as::<_, Source>("SELECT * FROM sources WHERE name = $1")
        .bind(name)
        .fetch_optional(pool)
        .await?;

    Ok(source)
}

/// Update source last_crawled_at timestamp
pub async fn update_last_crawled(pool: &DbPool, source_id: i32) -> Result<()> {
    sqlx::query("UPDATE sources SET last_crawled_at = NOW(), updated_at = NOW() WHERE id = $1")
        .bind(source_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Update source description
pub async fn update_description(pool: &DbPool, source_id: i32, description: &str) -> Result<()> {
    sqlx::query("UPDATE sources SET description = $2, updated_at = NOW() WHERE id = $1")
        .bind(source_id)
        .bind(description)
        .execute(pool)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // Tests require a running database - see integration tests
}
