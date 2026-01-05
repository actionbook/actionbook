//! Verify that documents have correct URL format with fragments
//!
//! Usage: cargo run --bin verify_url_format [source_id]
//!
//! Arguments:
//!   source_id  Optional source ID to check (defaults to latest)

use handbook_builder::db::create_pool_from_env;
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env
    dotenvy::dotenv().ok();

    println!("Connecting to database...");
    let pool = create_pool_from_env().await?;
    println!("✓ Connected to database\n");

    // Get source_id from command line args or find latest
    let source_id = if let Some(arg) = std::env::args().nth(1) {
        arg.parse::<i32>()
            .map_err(|_| format!("Invalid source_id: {}", arg))?
    } else {
        // Find latest source with 'any' category documents
        let row = sqlx::query(
            r#"
            SELECT DISTINCT d.source_id
            FROM documents d
            INNER JOIN sources s ON d.source_id = s.id
            WHERE d.url LIKE '%#handbook-%'
               OR d.url LIKE '%.md'
            ORDER BY d.created_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&pool)
        .await?;

        match row {
            Some(r) => {
                let id: i32 = r.get("source_id");
                println!("📌 No source_id provided, using latest: {}\n", id);
                id
            }
            None => {
                eprintln!("❌ No sources found with handbook documents");
                eprintln!("   Run: cargo run --bin create_real_test_task");
                std::process::exit(1);
            }
        }
    };

    println!("Checking documents for source_id={}...\n", source_id);

    let docs = sqlx::query(
        r#"
        SELECT
            d.id,
            d.url,
            d.title,
            d.url_hash,
            LENGTH(d.content_md) as content_length
        FROM documents d
        WHERE d.source_id = $1
        ORDER BY d.id
        "#,
    )
    .bind(source_id)
    .fetch_all(&pool)
    .await?;

    if docs.is_empty() {
        println!("❌ No documents found for source_id={}", source_id);
        return Ok(());
    }

    println!("Found {} documents:\n", docs.len());
    println!("{:-<100}", "");
    println!("{:<10} {:<50} {:<30}", "ID", "URL", "Title");
    println!("{:-<100}", "");

    for doc in &docs {
        let id: i32 = doc.get("id");
        let url: String = doc.get("url");
        let title: Option<String> = doc.get("title");
        let title_str = title.unwrap_or_else(|| "N/A".to_string());

        println!("{:<10} {:<50} {:<30}", id, url, title_str);
    }
    println!("{:-<100}\n", "");

    // Verify URL format
    println!("URL Format Verification:");
    println!("{:-<100}", "");

    let mut has_fragment = false;
    let mut has_md_suffix = false;
    let mut has_plain_url = false;

    for doc in &docs {
        let url: String = doc.get("url");

        if url.contains("#handbook-") {
            has_fragment = true;
            println!("✅ Fragment format: {}", url);
        } else if url.ends_with(".md") {
            has_md_suffix = true;
            println!("❌ .md suffix format: {}", url);
        } else {
            has_plain_url = true;
            println!("⚠️  Plain URL (no fragment): {}", url);
        }
    }

    println!("{:-<100}\n", "");

    // Summary
    println!("Summary:");
    if has_fragment && !has_md_suffix && !has_plain_url {
        println!("✅ All documents use fragment format correctly!");
        println!("✅ Fix is working as expected!");
    } else if has_md_suffix {
        println!("❌ Some documents still have .md suffix");
        println!("   This indicates the fix may not be applied or old data exists");
    } else if has_plain_url {
        println!("⚠️  Some documents use plain URL (no fragment)");
        println!("   This may cause url_hash conflicts");
    }

    // Check for url_hash conflicts
    println!("\nChecking for url_hash conflicts...");
    let conflicts = sqlx::query(
        r#"
        SELECT
            url_hash,
            COUNT(*) as count,
            STRING_AGG(title, ', ') as titles
        FROM documents
        WHERE source_id = $1
        GROUP BY url_hash
        HAVING COUNT(*) > 1
        "#,
    )
    .bind(source_id)
    .fetch_all(&pool)
    .await?;

    if conflicts.is_empty() {
        println!("✅ No url_hash conflicts detected!");
    } else {
        println!("❌ Found {} url_hash conflicts:", conflicts.len());
        for conflict in conflicts {
            let hash: String = conflict.get("url_hash");
            let count: i64 = conflict.get("count");
            let titles: Option<String> = conflict.get("titles");
            println!("  Hash: {}... (count: {}, titles: {})",
                &hash[..16], count, titles.unwrap_or_else(|| "N/A".to_string()));
        }
    }

    // Show chunk distribution
    println!("\nChunk Distribution:");
    println!("{:-<100}", "");

    let chunks = sqlx::query(
        r#"
        SELECT
            d.id as doc_id,
            d.title,
            COUNT(c.id) as chunk_count
        FROM documents d
        LEFT JOIN chunks c ON d.id = c.document_id
        WHERE d.source_id = $1
        GROUP BY d.id, d.title
        ORDER BY d.id
        "#,
    )
    .bind(source_id)
    .fetch_all(&pool)
    .await?;

    for chunk in chunks {
        let doc_id: i32 = chunk.get("doc_id");
        let title: Option<String> = chunk.get("title");
        let count: i64 = chunk.get("chunk_count");
        println!("  Document {}: {} - {} chunks",
            doc_id,
            title.unwrap_or_else(|| "N/A".to_string()),
            count
        );
    }

    println!("\n✓ Verification complete!");

    Ok(())
}
