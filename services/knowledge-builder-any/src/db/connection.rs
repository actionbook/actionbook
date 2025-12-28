//! Database connection management

use crate::error::Result;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

/// Type alias for the database pool
pub type DbPool = PgPool;

/// Create a new database connection pool
///
/// # Arguments
/// * `database_url` - PostgreSQL connection string
///
/// # Example
/// ```ignore
/// let pool = create_pool("postgres://user:pass@localhost/db").await?;
/// ```
pub async fn create_pool(database_url: &str) -> Result<DbPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(30))
        .connect(database_url)
        .await?;

    Ok(pool)
}

/// Create a pool from DATABASE_URL environment variable
pub async fn create_pool_from_env() -> Result<DbPool> {
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| crate::error::HandbookError::ConfigError("DATABASE_URL not set".to_string()))?;

    create_pool(&database_url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_create_pool() {
        dotenvy::dotenv().ok();
        let pool = create_pool_from_env().await;
        assert!(pool.is_ok());
    }
}
