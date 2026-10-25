use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Result;

/// Store a new session expiring at `expires_at` (unix seconds), pruning
/// expired ones in the same round trip.
pub async fn create(pool: &PgPool, user_id: Uuid, expires_at: u64) -> Result<Uuid> {
    Ok(sqlx::query_scalar!(
        r#"with pruned as (delete from sessions where expires_at < now())
           insert into sessions (user_id, expires_at) values ($1, to_timestamp($2))
           returning id"#,
        user_id,
        expires_at as f64,
    )
    .fetch_one(pool)
    .await?)
}

/// Delete one session, for logout.
pub async fn delete(pool: &PgPool, session_id: Uuid) -> Result<()> {
    sqlx::query!(r#"delete from sessions where id = $1"#, session_id)
        .execute(pool)
        .await?;
    Ok(())
}

