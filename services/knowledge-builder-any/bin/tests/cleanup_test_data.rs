//! Clean up test data from database
//!
//! Usage:
//!   cargo run --bin cleanup_test_data [source_id]
//!   cargo run --bin cleanup_test_data --all-test
//!
//! Options:
//!   source_id     Clean up specific source by ID
//!   --all-test    Clean up all test sources (contains 'test' in name)
//!   --latest      Clean up the latest created source
//!
//! This tool removes:
//! - Chunks associated with the source's documents
//! - Documents associated with the source
//! - Source versions
//! - Recording tasks for the source
//! - Build tasks for the source
//! - The source itself

use handbook_builder::db::create_pool_from_env;
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let pool = create_pool_from_env().await?;

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let source_ids = match args[1].as_str() {
        "--all-test" => {
            find_test_sources(&pool).await?
        }
        "--latest" => {
            if let Some(id) = find_latest_source(&pool).await? {
                vec![id]
            } else {
                eprintln!("❌ No sources found");
                return Ok(());
            }
        }
        arg => {
            vec![arg.parse::<i32>()
                .map_err(|_| format!("Invalid source_id: {}", arg))?]
        }
    };

    if source_ids.is_empty() {
        println!("✓ No test sources found to clean up");
        return Ok(());
    }

    // Show what will be deleted
    println!("📋 Sources to be deleted:");
    for source_id in &source_ids {
        show_source_info(&pool, *source_id).await?;
    }

    // Confirm deletion
    println!("\n⚠️  This will permanently delete the above data!");
    print!("Continue? (yes/no): ");
    use std::io::{self, Write};
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() != "yes" {
        println!("❌ Cancelled");
        return Ok(());
    }

    // Delete data
    for source_id in source_ids {
        cleanup_source(&pool, source_id).await?;
    }

    println!("\n✅ Cleanup complete!");
    Ok(())
}

fn print_usage() {
    println!("Usage:");
    println!("  cargo run --bin cleanup_test_data <source_id>");
    println!("  cargo run --bin cleanup_test_data --all-test");
    println!("  cargo run --bin cleanup_test_data --latest");
    println!();
    println!("Options:");
    println!("  source_id     Clean up specific source by ID");
    println!("  --all-test    Clean up all sources containing 'test' in name");
    println!("  --latest      Clean up the latest created source");
}

async fn find_test_sources(pool: &sqlx::PgPool) -> Result<Vec<i32>, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        r#"
        SELECT id FROM sources
        WHERE LOWER(name) LIKE '%test%'
           OR LOWER(base_url) LIKE '%example%'
           OR LOWER(base_url) LIKE '%httpbin%'
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|r| r.get("id")).collect())
}

async fn find_latest_source(pool: &sqlx::PgPool) -> Result<Option<i32>, Box<dyn std::error::Error>> {
    let row = sqlx::query(
        "SELECT id FROM sources ORDER BY created_at DESC LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| r.get("id")))
}

async fn show_source_info(pool: &sqlx::PgPool, source_id: i32) -> Result<(), Box<dyn std::error::Error>> {
    // Get source info
    let source = sqlx::query(
        "SELECT id, name, base_url, created_at FROM sources WHERE id = $1"
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await?;

    if source.is_none() {
        println!("  ⚠️  Source {} not found", source_id);
        return Ok(());
    }

    let source = source.unwrap();
    let name: String = source.get("name");
    let base_url: String = source.get("base_url");
    let created_at: chrono::DateTime<chrono::Utc> = source.get("created_at");

    println!("\n  Source ID: {}", source_id);
    println!("  Name: {}", name);
    println!("  URL: {}", base_url);
    println!("  Created: {}", created_at.format("%Y-%m-%d %H:%M:%S"));

    // Count related data
    let doc_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM documents WHERE source_id = $1"
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;

    let chunk_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM chunks c
        INNER JOIN documents d ON c.document_id = d.id
        WHERE d.source_id = $1
        "#
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;

    let task_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM build_tasks WHERE source_id = $1"
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;

    let version_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM source_versions WHERE source_id = $1"
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;

    println!("  └─ Documents: {}", doc_count);
    println!("     └─ Chunks: {}", chunk_count);
    println!("  └─ Build Tasks: {}", task_count);
    println!("  └─ Versions: {}", version_count);

    Ok(())
}

async fn cleanup_source(pool: &sqlx::PgPool, source_id: i32) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🗑️  Cleaning up source {}...", source_id);

    // Delete in reverse dependency order

    // 1. Delete chunks
    let chunk_result = sqlx::query(
        r#"
        DELETE FROM chunks
        WHERE document_id IN (
            SELECT id FROM documents WHERE source_id = $1
        )
        "#
    )
    .bind(source_id)
    .execute(pool)
    .await?;
    println!("  ✓ Deleted {} chunks", chunk_result.rows_affected());

    // 2. Delete recording_steps (if any)
    let steps_result = sqlx::query(
        r#"
        DELETE FROM recording_steps
        WHERE task_id IN (
            SELECT id FROM recording_tasks WHERE source_id = $1
        )
        "#
    )
    .bind(source_id)
    .execute(pool)
    .await?;
    if steps_result.rows_affected() > 0 {
        println!("  ✓ Deleted {} recording steps", steps_result.rows_affected());
    }

    // 3. Delete recording_tasks
    let rec_tasks_result = sqlx::query(
        "DELETE FROM recording_tasks WHERE source_id = $1"
    )
    .bind(source_id)
    .execute(pool)
    .await?;
    if rec_tasks_result.rows_affected() > 0 {
        println!("  ✓ Deleted {} recording tasks", rec_tasks_result.rows_affected());
    }

    // 4. Delete documents
    let doc_result = sqlx::query(
        "DELETE FROM documents WHERE source_id = $1"
    )
    .bind(source_id)
    .execute(pool)
    .await?;
    println!("  ✓ Deleted {} documents", doc_result.rows_affected());

    // 5. Delete source_versions
    let version_result = sqlx::query(
        "DELETE FROM source_versions WHERE source_id = $1"
    )
    .bind(source_id)
    .execute(pool)
    .await?;
    if version_result.rows_affected() > 0 {
        println!("  ✓ Deleted {} versions", version_result.rows_affected());
    }

    // 6. Delete build_tasks
    let build_task_result = sqlx::query(
        "DELETE FROM build_tasks WHERE source_id = $1"
    )
    .bind(source_id)
    .execute(pool)
    .await?;
    if build_task_result.rows_affected() > 0 {
        println!("  ✓ Deleted {} build tasks", build_task_result.rows_affected());
    }

    // 7. Finally, delete the source
    sqlx::query(
        "DELETE FROM sources WHERE id = $1"
    )
    .bind(source_id)
    .execute(pool)
    .await?;
    println!("  ✓ Deleted source");

    println!("✓ Source {} cleaned up successfully", source_id);
    Ok(())
}
