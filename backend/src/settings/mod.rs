use sqlx::PgPool;

use crate::error::AppResult;

/// 访问口令哈希
pub const ACCESS_CODE_HASH: &str = "access_code_hash";
/// 令牌签名密钥
pub const SIGNING_KEY: &str = "signing_key";

pub async fn get(pool: &PgPool, key: &str) -> AppResult<Option<String>> {
    let value = sqlx::query_scalar::<_, String>("SELECT value FROM app_settings WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(value)
}

pub async fn set(pool: &PgPool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES ($1, $2) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}
