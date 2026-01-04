//! Create a test task for a real, small website
//!
//! Usage: cargo run --bin create_real_test_task

use handbook_builder::db::create_pool_from_env;
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env
    dotenvy::dotenv().ok();

    println!("Connecting to database...");
    let pool = create_pool_from_env().await?;
    println!("✓ Connected to database");

    // Use httpbin.org - a simple HTTP testing service
    let test_url = "https://httpbin.org";
    println!("\nCreating test task for: {}", test_url);

    // Check if source already exists
    let existing = sqlx::query(
        "SELECT id, name FROM sources WHERE base_url = $1"
    )
    .bind(test_url)
    .fetch_optional(&pool)
    .await?;

    if let Some(row) = existing {
        let source_id: i32 = row.get("id");
        let source_name: String = row.get("name");
        println!("\n⚠️  Source already exists:");
        println!("  ID: {}", source_id);
        println!("  Name: {}", source_name);
        println!("\n  To test with this source, you can:");
        println!("  1. Delete existing documents for this source");
        println!("  2. Or use a different URL");
        return Ok(());
    }

    // Create task
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
    .bind("HTTPBin Test Service")
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
