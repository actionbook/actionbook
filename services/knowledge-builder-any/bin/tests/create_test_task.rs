//! Create a test build_task for testing knowledge-builder-any worker
//!
//! Usage: cargo run --bin create_test_task

use handbook_builder::db::create_pool_from_env;
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env
    dotenvy::dotenv().ok();

    println!("Connecting to database...");
    let pool = create_pool_from_env().await?;
    println!("✓ Connected to database");

    // Insert a test task
    let test_url = "https://example.com";
    println!("\nCreating test task for: {}", test_url);

    let result = sqlx::query(
        r#"
        INSERT INTO build_tasks (
            source_url,
            source_name,
            source_category,
            stage,
            stage_status,
            created_at,
            updated_at
        ) VALUES (
            $1,
            $2,
            'any',
            'init',
            'pending',
            NOW(),
            NOW()
        )
        RETURNING id, source_url, stage, stage_status
        "#,
    )
    .bind(test_url)
    .bind("Example Domain Test")
    .fetch_one(&pool)
    .await?;

    let task_id: i32 = result.get("id");
    let url: String = result.get("source_url");
    let stage: String = result.get("stage");
    let status: String = result.get("stage_status");

    println!("✓ Test task created:");
    println!("  ID: {}", task_id);
    println!("  URL: {}", url);
    println!("  Stage: {}", stage);
    println!("  Status: {}", status);

    // Check pending tasks count
    let count_row = sqlx::query(
        r#"
        SELECT COUNT(*) as count FROM build_tasks
        WHERE source_category = 'any'
          AND stage = 'init'
          AND stage_status = 'pending'
        "#,
    )
    .fetch_one(&pool)
    .await?;

    let pending_count: i64 = count_row.get("count");
    println!("\nTotal pending tasks: {}", pending_count);

    println!("\nNext steps:");
    println!("  1. Run worker: cargo run --release -- worker --once");
    println!("  2. Check documents: psql $DATABASE_URL -f .docs/verify-url-fix.sql");
    println!("\n💡 To clean up test data after testing:");
    println!("  cargo run --bin cleanup_test_data -- --latest");

    Ok(())
}
