use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Result;

/// Store a new session living `lifetime_seconds` until its next refresh,
/// pruning expired ones in the same round trip.
pub async fn create(
    pool: &PgPool,
    user_id: Uuid,
    refresh_token_hash: &str,
    remember_me: bool,
    lifetime_seconds: i32,
) -> Result<Uuid> {
    Ok(sqlx::query_scalar!(
        r#"with pruned as (delete from sessions where expires_at < now())
           insert into sessions (user_id, refresh_token_hash, remember_me, expires_at)
           values ($1, $2, $3, now() + make_interval(secs => $4::integer))
           returning id"#,
        user_id,
        refresh_token_hash,
        remember_me,
        lifetime_seconds,
    )
    .fetch_one(pool)
    .await?)
}

/// A session found by its refresh token.
pub struct RefreshedSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub remember_me: bool,
}

/// Swap a live session's refresh token for a new one and push its expiry
/// back by its lifetime. None when no live session holds the old token.
pub async fn refresh(
    pool: &PgPool,
    refresh_token_hash: &str,
    new_refresh_token_hash: &str,
    remembered_lifetime_seconds: i32,
    lifetime_seconds: i32,
) -> Result<Option<RefreshedSession>> {
    Ok(sqlx::query_as!(
        RefreshedSession,
        r#"update sessions
           set previous_refresh_token_hash = refresh_token_hash,
               refresh_token_hash = $2,
               refreshed_at = now(),
               expires_at = now() + make_interval(secs => case when remember_me then $3::integer else $4::integer end)
           where refresh_token_hash = $1 and expires_at > now()
           returning id, user_id, remember_me"#,
        refresh_token_hash,
        new_refresh_token_hash,
        remembered_lifetime_seconds,
        lifetime_seconds,
    )
    .fetch_optional(pool)
    .await?)
}

/// Delete the session whose last refresh spent `refresh_token_hash`: a
/// replay means the token leaked. Replays within a minute are spared, as
/// they are concurrent refreshes from the same client (several tabs).
pub async fn revoke_by_spent_refresh_token(pool: &PgPool, refresh_token_hash: &str) -> Result<()> {
    sqlx::query!(
        r#"delete from sessions
           where previous_refresh_token_hash = $1 and refreshed_at < now() - interval '1 minute'"#,
        refresh_token_hash,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete one session, for logout.
pub async fn delete(pool: &PgPool, session_id: Uuid) -> Result<()> {
    sqlx::query!(r#"delete from sessions where id = $1"#, session_id)
        .execute(pool)
        .await?;
    Ok(())
}
