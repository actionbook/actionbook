//! Create a unique test build_task for testing knowledge-builder-any worker
//!
//! Usage: cargo run --bin create_unique_test_task

use handbook_builder::db::create_pool_from_env;
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env
    dotenvy::dotenv().ok();

    println!("Connecting to database...");
    let pool = create_pool_from_env().await?;
    println!("✓ Connected to database");

    // Use a unique test URL with timestamp
    let timestamp = chrono::Utc::now().timestamp();
    let test_url = format!("https://example-{}.com", timestamp);
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
    .bind(&test_url)
    .bind(format!("Test Site {}", timestamp))
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

    println!("\n✓ Task ready for worker processing");
    println!("  Run: cargo run --release -- worker --once");
    println!("\n💡 To clean up test data after testing:");
    println!("  cargo run --bin cleanup_test_data -- --latest");

    Ok(())
}
