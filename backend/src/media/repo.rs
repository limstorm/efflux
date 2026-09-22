use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::moments::models::MediaRow;

pub struct NewMedia {
    pub kind: String,
    pub role: String,
    pub file_path: String,
    pub thumb_path: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
}

const COLUMNS: &str = "id, moment_id, kind, role, file_path, thumb_path, mime_type, \
                       size_bytes, width, height, duration_ms, position";

pub async fn insert(pool: &PgPool, media: NewMedia) -> AppResult<MediaRow> {
    let sql = format!(
        "INSERT INTO media (kind, role, file_path, thumb_path, mime_type, size_bytes, width, height, duration_ms) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {COLUMNS}"
    );
    let row = sqlx::query_as::<_, MediaRow>(&sql)
        .bind(media.kind)
        .bind(media.role)
        .bind(media.file_path)
        .bind(media.thumb_path)
        .bind(media.mime_type)
        .bind(media.size_bytes)
        .bind(media.width)
        .bind(media.height)
        .bind(media.duration_ms)
        .fetch_one(pool)
        .await?;
    Ok(row)
}

pub async fn find(pool: &PgPool, id: Uuid) -> AppResult<Option<MediaRow>> {
    let sql = format!("SELECT {COLUMNS} FROM media WHERE id = $1");
    let row = sqlx::query_as::<_, MediaRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// 只允许删除尚未归属任何点滴的媒体（防止误删正在使用的内容）。
pub async fn delete_orphan(pool: &PgPool, id: Uuid) -> AppResult<bool> {
    let result = sqlx::query("DELETE FROM media WHERE id = $1 AND moment_id IS NULL")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
