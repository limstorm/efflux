use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::config::Config;

pub async fn connect(config: &Config) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(8))
        .idle_timeout(Duration::from_secs(600))
        .connect(&config.database_url)
        .await?;

    Ok(pool)
}

/// Docker Compose 下数据库可能比应用晚就绪，这里做有限重试。
pub async fn connect_with_retry(config: &Config) -> anyhow::Result<PgPool> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match connect(config).await {
            Ok(pool) => return Ok(pool),
            Err(err) if attempt < 30 => {
                tracing::warn!(attempt, error = %err, "数据库尚未就绪，1 秒后重试");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(err) => return Err(err),
        }
    }
}

pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
