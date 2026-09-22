use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::models::{MediaRow, MomentRow, TagWithCount};
use crate::error::AppResult;

/// 过滤条件（q 已转义 LIKE 元字符）
#[derive(Debug, Default, Clone)]
pub struct MomentFilter {
    pub q: Option<String>,
    pub tags: Option<Vec<String>>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub ascending: bool,
    pub limit: i64,
    pub offset: i64,
}

/// 写入地点时的一整套。更新时作为整体替换：
/// 传了名字就一起覆盖，没传就原样不动。
#[derive(Debug, Clone, Default)]
pub struct PlaceInput {
    pub name: String,
    pub city: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// 转义 LIKE 模式中的通配符，避免用户输入的 % / _ 造成意外匹配
pub fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

// 关键词一个框搞定：标题、正文、地点名、城市都算在内——
// 想按地方翻找，直接在里面写「杭州」就是了，不必再多一个输入框
const FILTER_CLAUSE: &str = r#"
    ($1::text IS NULL
        OR title ILIKE '%' || $1 || '%' ESCAPE '\'
        OR content ILIKE '%' || $1 || '%' ESCAPE '\'
        OR location_name ILIKE '%' || $1 || '%' ESCAPE '\'
        OR city ILIKE '%' || $1 || '%' ESCAPE '\')
    AND ($2::timestamptz IS NULL OR happened_at >= $2)
    AND ($3::timestamptz IS NULL OR happened_at <= $3)
    AND ($4::text[] IS NULL OR EXISTS (
        SELECT 1 FROM moment_tags mt JOIN tags t ON t.id = mt.tag_id
        WHERE mt.moment_id = moments.id AND t.name = ANY($4)
    ))
"#;

const MOMENT_COLUMNS: &str = "id, title, content, happened_at, created_at, updated_at, \
                              location_name, city, latitude, longitude, ambient, share_token";

pub async fn list(
    pool: &PgPool,
    filter: &MomentFilter,
) -> AppResult<(Vec<MomentRow>, i64)> {
    let direction = if filter.ascending { "ASC" } else { "DESC" };
    let sql = format!(
        "SELECT {MOMENT_COLUMNS} FROM moments WHERE {FILTER_CLAUSE} \
         ORDER BY happened_at {direction}, created_at {direction} LIMIT $5 OFFSET $6"
    );
    let rows = sqlx::query_as::<_, MomentRow>(&sql)
        .bind(filter.q.clone())
        .bind(filter.from)
        .bind(filter.to)
        .bind(filter.tags.clone())
        .bind(filter.limit)
        .bind(filter.offset)
        .fetch_all(pool)
        .await?;

    let count_sql = format!("SELECT count(*) FROM moments WHERE {FILTER_CLAUSE}");
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(filter.q.clone())
        .bind(filter.from)
        .bind(filter.to)
        .bind(filter.tags.clone())
        .fetch_one(pool)
        .await?;

    Ok((rows, total))
}

pub async fn find(pool: &PgPool, id: Uuid) -> AppResult<Option<MomentRow>> {
    let sql = format!("SELECT {MOMENT_COLUMNS} FROM moments WHERE id = $1");
    let row = sqlx::query_as::<_, MomentRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn insert(
    conn: &mut PgConnection,
    title: &str,
    content: &str,
    happened_at: DateTime<Utc>,
    place: &PlaceInput,
    ambient: Option<&str>,
) -> AppResult<MomentRow> {
    let sql = format!(
        "INSERT INTO moments (title, content, happened_at, location_name, city, latitude, longitude, ambient) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {MOMENT_COLUMNS}"
    );
    let row = sqlx::query_as::<_, MomentRow>(&sql)
        .bind(title)
        .bind(content)
        .bind(happened_at)
        .bind(&place.name)
        .bind(&place.city)
        .bind(place.latitude)
        .bind(place.longitude)
        .bind(ambient)
        .fetch_one(conn)
        .await?;
    Ok(row)
}

pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    title: Option<&str>,
    content: Option<&str>,
    happened_at: Option<DateTime<Utc>>,
    place: Option<&PlaceInput>,
    ambient: Option<&str>,
) -> AppResult<Option<MomentRow>> {
    // 地点是整体替换：给名字就连城市、坐标一起覆盖，没给就原样不动。
    // 氛围音也是三条路：不给（NULL）不动、空串清空、名字照写。
    let sql = format!(
        "UPDATE moments SET \
            title = COALESCE($2, title), \
            content = COALESCE($3, content), \
            happened_at = COALESCE($4, happened_at), \
            location_name = COALESCE($5, location_name), \
            city = CASE WHEN $5 IS NULL THEN city ELSE $6 END, \
            latitude = CASE WHEN $5 IS NULL THEN latitude ELSE $7 END, \
            longitude = CASE WHEN $5 IS NULL THEN longitude ELSE $8 END, \
            ambient = CASE WHEN $9 IS NULL THEN ambient ELSE NULLIF($9, '') END, \
            updated_at = now() \
         WHERE id = $1 RETURNING {MOMENT_COLUMNS}"
    );
    let (name, city, latitude, longitude) = match place {
        Some(place) => (
            Some(place.name.as_str()),
            Some(place.city.as_str()),
            place.latitude,
            place.longitude,
        ),
        None => (None, None, None, None),
    };
    let row = sqlx::query_as::<_, MomentRow>(&sql)
        .bind(id)
        .bind(title)
        .bind(content)
        .bind(happened_at)
        .bind(name)
        .bind(city)
        .bind(latitude)
        .bind(longitude)
        .bind(ambient)
        .fetch_optional(conn)
        .await?;
    Ok(row)
}

/// 写入分享凭证。已经有就原样留着——分享出去的链接不该因为再点一次而换掉。
pub async fn set_share_token(pool: &PgPool, id: Uuid, token: &str) -> AppResult<Option<String>> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "UPDATE moments SET share_token = COALESCE(share_token, $2) \
         WHERE id = $1 RETURNING share_token",
    )
    .bind(id)
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(token,)| token))
}

/// 收回分享：清掉凭证，旧链接立刻失效
pub async fn clear_share_token(pool: &PgPool, id: Uuid) -> AppResult<bool> {
    let result = sqlx::query("UPDATE moments SET share_token = NULL WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// 按凭证取一条记忆。查不到就是链接失效，或者从来没分享过
pub async fn find_by_share_token(pool: &PgPool, token: &str) -> AppResult<Option<MomentRow>> {
    let sql = format!("SELECT {MOMENT_COLUMNS} FROM moments WHERE share_token = $1");
    let row = sqlx::query_as::<_, MomentRow>(&sql)
        .bind(token)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// 删除一条记忆，并在删除留痕里记一笔。
///
/// 两件事必须一起成功：漏了留痕，增量备份恢复时这条记忆会「复活」——
/// 增量包不带已删的东西，只能靠这份留痕知道它被删过。
pub async fn delete(pool: &PgPool, id: Uuid) -> AppResult<bool> {
    let mut tx = pool.begin().await?;

    let result = sqlx::query("DELETE FROM moments WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO deletions (id, table_name) VALUES ($1, 'moments') ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// 打包时取用：某个时刻之后删掉的东西。传 None 就取全部
pub async fn deletions_since(
    pool: &PgPool,
    since: Option<DateTime<Utc>>,
) -> AppResult<Vec<(Uuid, String, DateTime<Utc>)>> {
    let rows = match since {
        Some(since) => {
            sqlx::query_as(
                "SELECT id, table_name, deleted_at FROM deletions \
                 WHERE deleted_at > $1 ORDER BY deleted_at",
            )
            .bind(since)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as(
                "SELECT id, table_name, deleted_at FROM deletions ORDER BY deleted_at",
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows)
}

/// 新的基准包落地后清空留痕：完整快照本身就是真值，历史不再需要
pub async fn clear_deletions(pool: &PgPool) -> AppResult<u64> {
    let result = sqlx::query("DELETE FROM deletions").execute(pool).await?;
    Ok(result.rows_affected())
}

pub async fn stats(pool: &PgPool) -> AppResult<(i64, Option<DateTime<Utc>>, Option<DateTime<Utc>>, i64)> {
    let row: (i64, Option<DateTime<Utc>>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT count(*), min(happened_at), max(happened_at) FROM moments",
    )
    .fetch_one(pool)
    .await?;

    let tag_count: i64 = sqlx::query_scalar("SELECT count(*) FROM tags")
        .fetch_one(pool)
        .await?;

    Ok((row.0, row.1, row.2, tag_count))
}

// ---------------------------------------------------------------- 关联数据

pub async fn media_for_moments(pool: &PgPool, ids: &[Uuid]) -> AppResult<Vec<MediaRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = "SELECT id, moment_id, kind, role, file_path, thumb_path, mime_type, \
               size_bytes, width, height, duration_ms, position \
               FROM media WHERE moment_id = ANY($1) ORDER BY position, created_at";
    let rows = sqlx::query_as::<_, MediaRow>(sql)
        .bind(ids)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn media_for_moment(pool: &PgPool, id: Uuid) -> AppResult<Vec<MediaRow>> {
    let sql = "SELECT id, moment_id, kind, role, file_path, thumb_path, mime_type, \
               size_bytes, width, height, duration_ms, position \
               FROM media WHERE moment_id = $1 ORDER BY position, created_at";
    let rows = sqlx::query_as::<_, MediaRow>(sql)
        .bind(id)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn tags_for_moments(pool: &PgPool, ids: &[Uuid]) -> AppResult<Vec<(Uuid, String)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT mt.moment_id, t.name FROM moment_tags mt \
         JOIN tags t ON t.id = mt.tag_id WHERE mt.moment_id = ANY($1) ORDER BY t.name",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn tags_for_moment(pool: &PgPool, id: Uuid) -> AppResult<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT t.name FROM moment_tags mt JOIN tags t ON t.id = mt.tag_id \
         WHERE mt.moment_id = $1 ORDER BY t.name",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn all_tags(pool: &PgPool) -> AppResult<Vec<TagWithCount>> {
    // 只返回仍在被使用的标签，避免搜索页出现空标签
    let rows = sqlx::query_as::<_, TagWithCount>(
        "SELECT t.name, count(mt.moment_id) AS count FROM tags t \
         JOIN moment_tags mt ON mt.tag_id = t.id \
         GROUP BY t.name HAVING count(mt.moment_id) > 0 \
         ORDER BY count DESC, t.name ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 删除点滴前先取出它的媒体文件路径，用于随后清理磁盘。
pub async fn files_for_moment(pool: &PgPool, id: Uuid) -> AppResult<Vec<(String, Option<String>)>> {
    let rows = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT file_path, thumb_path FROM media WHERE moment_id = $1",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 清理不再被任何点滴引用的标签。
pub async fn purge_unused_tags(pool: &PgPool) -> AppResult<u64> {
    let result = sqlx::query(
        "DELETE FROM tags t WHERE NOT EXISTS (SELECT 1 FROM moment_tags mt WHERE mt.tag_id = t.id)",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// 标签 get-or-create，返回对应 id（保持输入顺序）。
pub async fn upsert_tags(conn: &mut PgConnection, names: &[String]) -> AppResult<Vec<Uuid>> {
    let mut ids = Vec::with_capacity(names.len());
    for name in names {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO tags (name) VALUES ($1) \
             ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name RETURNING id",
        )
        .bind(name)
        .fetch_one(&mut *conn)
        .await?;
        ids.push(id);
    }
    Ok(ids)
}

pub async fn link_tags(conn: &mut PgConnection, moment_id: Uuid, tag_ids: &[Uuid]) -> AppResult<()> {
    if tag_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO moment_tags (moment_id, tag_id) \
         SELECT $1, unnest($2::uuid[]) ON CONFLICT DO NOTHING",
    )
    .bind(moment_id)
    .bind(tag_ids)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn clear_tags(conn: &mut PgConnection, moment_id: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM moment_tags WHERE moment_id = $1")
        .bind(moment_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// 把先上传的媒体认领到某条点滴上（只能认领尚未归属的媒体）。
pub async fn attach_media(
    conn: &mut PgConnection,
    moment_id: Uuid,
    media_ids: &[Uuid],
) -> AppResult<u64> {
    if media_ids.is_empty() {
        return Ok(0);
    }
    let result = sqlx::query(
        "UPDATE media SET \
            moment_id = $1, \
            role = CASE WHEN kind = 'audio' THEN 'music' ELSE 'attachment' END, \
            position = COALESCE(array_position($2::uuid[], id) - 1, 0) \
         WHERE id = ANY($2) AND (moment_id IS NULL OR moment_id = $1)",
    )
    .bind(moment_id)
    .bind(media_ids)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected())
}

pub async fn detach_media(conn: &mut PgConnection, moment_id: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE media SET moment_id = NULL WHERE moment_id = $1")
        .bind(moment_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// 回收长期未被任何点滴认领的孤儿媒体（上传后中途放弃的记录）。
pub async fn purge_orphan_media(
    pool: &PgPool,
    older_than_hours: i32,
) -> AppResult<Vec<(String, Option<String>)>> {
    let paths = sqlx::query_as::<_, (String, Option<String>)>(
        "DELETE FROM media WHERE moment_id IS NULL \
         AND created_at < now() - make_interval(hours => $1) \
         RETURNING file_path, thumb_path",
    )
    .bind(older_than_hours)
    .fetch_all(pool)
    .await?;
    Ok(paths)
}
