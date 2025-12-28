//! Build tasks database operations

use crate::db::models::BuildTask;
use crate::db::DbPool;
use crate::error::Result;
use sqlx::Row;

/// Fetch a pending task for knowledge build (source_category = 'any')
///
/// Note: `SELECT ... FOR UPDATE` is only effective within an explicit transaction.
/// Prefer `claim_next_pending_task()` for atomic claiming in multi-worker setups.
pub async fn fetch_pending_task(pool: &DbPool) -> Result<Option<BuildTask>> {
    let task = sqlx::query_as::<_, BuildTask>(
        r#"
        SELECT * FROM build_tasks
        WHERE source_category = 'any'
          AND stage = 'init'
          AND stage_status = 'pending'
        ORDER BY created_at ASC
        LIMIT 1
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(task)
}

/// Atomically claim the next pending task and return it.
///
/// This is safe for concurrent workers without requiring an explicit transaction.
pub async fn claim_next_pending_task(pool: &DbPool) -> Result<Option<BuildTask>> {
    let task = sqlx::query_as::<_, BuildTask>(
        r#"
        WITH next_task AS (
            SELECT id FROM build_tasks
            WHERE source_category = 'any'
              AND stage = 'init'
              AND stage_status = 'pending'
            ORDER BY created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE build_tasks
        SET stage = 'knowledge_build',
            stage_status = 'running',
            knowledge_started_at = NOW(),
            updated_at = NOW()
        WHERE id = (SELECT id FROM next_task)
        RETURNING *
        "#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(task)
}

/// Claim a task by updating its status to running
pub async fn claim_task(pool: &DbPool, task_id: i32) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE build_tasks
        SET stage = 'knowledge_build',
            stage_status = 'running',
            knowledge_started_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
          AND stage = 'init'
          AND stage_status = 'pending'
        "#,
    )
    .bind(task_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Complete a task successfully
///
/// Updates stage_status to 'completed' so action-builder can pick it up
/// Action-builder will look for: stage='knowledge_build' AND stage_status='completed'
pub async fn complete_task(pool: &DbPool, task_id: i32, source_id: i32) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE build_tasks
        SET stage_status = 'completed',
            source_id = $2,
            knowledge_completed_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(task_id)
    .bind(source_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark a task as errored
pub async fn error_task(pool: &DbPool, task_id: i32, error_msg: &str) -> Result<()> {
    // Store error in config.last_error
    let error_json = serde_json::json!(error_msg);

    sqlx::query(
        r#"
        UPDATE build_tasks
        SET stage_status = 'error',
            config = jsonb_set(
                COALESCE(config, '{}'),
                '{last_error}',
                $2::jsonb
            ),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(task_id)
    .bind(error_json)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get a task by ID
pub async fn get_task_by_id(pool: &DbPool, task_id: i32) -> Result<Option<BuildTask>> {
    let task = sqlx::query_as::<_, BuildTask>("SELECT * FROM build_tasks WHERE id = $1")
        .bind(task_id)
        .fetch_optional(pool)
        .await?;

    Ok(task)
}

/// Count pending tasks for monitoring
pub async fn count_pending_tasks(pool: &DbPool) -> Result<i64> {
    let row = sqlx::query(
        r#"
        SELECT COUNT(*) as count FROM build_tasks
        WHERE source_category = 'any'
          AND stage = 'init'
          AND stage_status = 'pending'
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(row.get("count"))
}

#[cfg(test)]
mod tests {
    // Tests require a running database - see integration tests
}
