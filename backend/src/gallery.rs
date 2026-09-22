//! 图集：把所有图片按时间摊开，按自动标签检索。
//!
//! 数据源就是 media 表里 kind = 'image' 的行，时间取所属点滴的
//! happened_at（还没归属的用上传时间），这样图集和光河的时间轴是一致的。

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const DEFAULT_LIMIT: i64 = 60;
const MAX_LIMIT: i64 = 200;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryQuery {
    /// 逗号分隔的标签 key，要求同时命中
    pub tags: Option<String>,
    /// 关键词：匹配标题、正文、标签名
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryTag {
    pub key: String,
    pub name: String,
    pub group: String,
    pub score: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryItem {
    pub id: Uuid,
    pub kind: String,
    pub url: String,
    pub thumb_url: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub moment_id: Option<Uuid>,
    pub title: String,
    pub happened_at: DateTime<Utc>,
    pub tags: Vec<GalleryTag>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryResponse {
    pub items: Vec<GalleryItem>,
    pub total: i64,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagStat {
    pub key: String,
    pub name: String,
    pub group: String,
    pub count: i64,
}

fn file_url(path: &str) -> String {
    format!("/files/{}", path.trim_start_matches('/'))
}

// ---------------------------------------------------------------- 找相似

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarItem {
    pub id: Uuid,
    pub kind: String,
    pub url: String,
    pub thumb_url: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub moment_id: Option<Uuid>,
    pub title: String,
    pub happened_at: DateTime<Utc>,
    /// 与这张图的余弦相似度，越接近 1 越像
    pub score: f32,
}

/// `GET /api/gallery/{id}/similar?limit=12`
///
/// 拿这张图的 CLIP 向量去和库里所有向量比。没有 pgvector，比较就在内存里做：
/// 几万张图全读出来也就是几毫秒的乘法，够用，还省掉一个部署依赖。
///
/// 没算过向量的图会返回一句提示——多半是自动打标还没轮到它，
/// 或者这台部署压根没开 EFFLUX_TAGGER。
pub async fn similar(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<SimilarQuery>,
) -> AppResult<Json<Vec<SimilarItem>>> {
    let limit = query.limit.unwrap_or(12).clamp(1, 60);

    let target: Option<(Vec<u8>, i32)> =
        sqlx::query_as("SELECT vec, dim FROM media_embeddings WHERE media_id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;

    let Some((bytes, dim)) = target else {
        return Err(AppError::validation(
            "这张图还没有向量，等自动打标轮到它，或者这台部署没开自动打标",
        ));
    };

    let Some(vector) = crate::embedding::from_bytes(&bytes) else {
        return Err(AppError::Internal(anyhow::anyhow!("向量数据坏了，长度不是 4 的倍数")));
    };

    let rows = sqlx::query(
        "SELECT e.media_id, e.vec, m.kind, m.file_path, m.thumb_path, m.width, m.height, \
                m.moment_id, m.created_at, \
                COALESCE(mo.title, '') AS title, \
                COALESCE(mo.happened_at, m.created_at) AS happened_at \
         FROM media_embeddings e \
         JOIN media m ON m.id = e.media_id \
         LEFT JOIN moments mo ON mo.id = m.moment_id \
         WHERE e.dim = $1 AND e.media_id <> $2",
    )
    .bind(dim)
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let mut scored: Vec<(f32, &sqlx::postgres::PgRow)> = rows
        .iter()
        .filter_map(|row| {
            let other: Vec<u8> = row.get("vec");
            let other = crate::embedding::from_bytes(&other)?;
            let score = crate::embedding::cosine(&vector, &other)?;
            Some((score, row))
        })
        .collect();

    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.truncate(limit as usize);

    let items = scored
        .into_iter()
        .map(|(score, row)| {
            let file_path: String = row.get("file_path");
            let thumb_path: Option<String> = row.get("thumb_path");
            SimilarItem {
                id: row.get("media_id"),
                kind: row.get("kind"),
                url: file_url(&file_path),
                thumb_url: thumb_path.as_deref().map(file_url),
                width: row.get("width"),
                height: row.get("height"),
                moment_id: row.get("moment_id"),
                title: row.get("title"),
                happened_at: row.get("happened_at"),
                score,
            }
        })
        .collect();

    Ok(Json(items))
}

fn parse_tags(raw: Option<&str>) -> Option<Vec<String>> {
    let list: Vec<String> = raw
        .unwrap_or_default()
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect();
    if list.is_empty() {
        None
    } else {
        Some(list)
    }
}

fn parse_q(raw: Option<&str>) -> Option<String> {
    raw.map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 同一个 WHERE，列表与计数共用，免得两边条件写岔了
const WHERE_CLAUSE: &str = r#"
    m.kind IN ('image', 'video')
    AND ($1::text[] IS NULL OR NOT EXISTS (
        SELECT 1 FROM unnest($1::text[]) AS want(key)
        WHERE NOT EXISTS (
            SELECT 1 FROM media_tags mt
            WHERE mt.media_id = m.id AND mt.tag_key = want.key
        )
    ))
    AND ($2::text IS NULL OR (
        mo.title ILIKE '%' || $2 || '%'
        OR mo.content ILIKE '%' || $2 || '%'
        OR EXISTS (
            SELECT 1 FROM media_tags mt2
            WHERE mt2.media_id = m.id AND mt2.tag_name ILIKE '%' || $2 || '%'
        )
    ))
"#;

/// `GET /api/gallery?tags=cat,night&q=海边&limit=60&offset=0`
pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<GalleryQuery>,
) -> AppResult<Json<GalleryResponse>> {
    let tags = parse_tags(query.tags.as_deref());
    let q = parse_q(query.q.as_deref());
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);

    let count_sql = format!(
        "SELECT COUNT(*) AS total FROM media m LEFT JOIN moments mo ON mo.id = m.moment_id WHERE {WHERE_CLAUSE}"
    );
    let total: i64 = sqlx::query(&count_sql)
        .bind(tags.as_ref())
        .bind(q.as_ref())
        .fetch_one(&state.pool)
        .await?
        .get("total");

    let list_sql = format!(
        "SELECT m.id, m.kind, m.file_path, m.thumb_path, m.width, m.height, m.moment_id,
                COALESCE(mo.title, '') AS title,
                COALESCE(mo.happened_at, m.created_at) AS happened_at
         FROM media m
         LEFT JOIN moments mo ON mo.id = m.moment_id
         WHERE {WHERE_CLAUSE}
         ORDER BY happened_at DESC, m.id DESC
         LIMIT $3 OFFSET $4"
    );

    let rows = sqlx::query(&list_sql)
        .bind(tags.as_ref())
        .bind(q.as_ref())
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?;

    let mut items: Vec<GalleryItem> = Vec::with_capacity(rows.len());
    for row in &rows {
        let file_path: String = row.get("file_path");
        let thumb_path: Option<String> = row.get("thumb_path");
        items.push(GalleryItem {
            id: row.get("id"),
            kind: row.get("kind"),
            url: file_url(&file_path),
            thumb_url: thumb_path.as_deref().map(file_url),
            width: row.get("width"),
            height: row.get("height"),
            moment_id: row.get("moment_id"),
            title: row.get("title"),
            happened_at: row.get("happened_at"),
            tags: Vec::new(),
        });
    }

    // 一批取回标签，避免每张图各查一次
    if !items.is_empty() {
        let ids: Vec<Uuid> = items.iter().map(|item| item.id).collect();
        let tag_rows = sqlx::query(
            "SELECT media_id, tag_key, tag_name, tag_group, score FROM media_tags
             WHERE media_id = ANY($1)
             ORDER BY score DESC",
        )
        .bind(&ids)
        .fetch_all(&state.pool)
        .await?;

        let mut grouped: std::collections::HashMap<Uuid, Vec<GalleryTag>> =
            std::collections::HashMap::new();
        for row in tag_rows {
            let media_id: Uuid = row.get("media_id");
            grouped.entry(media_id).or_default().push(GalleryTag {
                key: row.get("tag_key"),
                name: row.get("tag_name"),
                group: row.get("tag_group"),
                score: row.get("score"),
            });
        }
        for item in items.iter_mut() {
            if let Some(found) = grouped.remove(&item.id) {
                item.tags = found;
            }
        }
    }

    let has_more = offset + (items.len() as i64) < total;
    Ok(Json(GalleryResponse {
        items,
        total,
        has_more,
    }))
}

/// `GET /api/gallery/tags` 每个标签下有多少张图，用来铺筛选条
pub async fn tags(State(state): State<AppState>) -> AppResult<Json<Vec<TagStat>>> {
    let rows = sqlx::query(
        "SELECT t.tag_key, t.tag_name, t.tag_group, COUNT(*) AS count
         FROM media_tags t
         JOIN media m ON m.id = t.media_id
         WHERE m.kind IN ('image', 'video')
         GROUP BY t.tag_key, t.tag_name, t.tag_group
         ORDER BY count DESC, t.tag_name ASC",
    )
    .fetch_all(&state.pool)
    .await?;

    let list: Vec<TagStat> = rows
        .into_iter()
        .map(|row| TagStat {
            key: row.get("tag_key"),
            name: row.get("tag_name"),
            group: row.get("tag_group"),
            count: row.get("count"),
        })
        .collect();

    Ok(Json(list))
}

/// 供 media 列表补标签用（图集以外的场景）
pub async fn tags_for(pool: &PgPool, ids: &[Uuid]) -> sqlx::Result<Vec<(Uuid, String, f32)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT media_id, tag_name, score FROM media_tags WHERE media_id = ANY($1) ORDER BY score DESC",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| (row.get("media_id"), row.get("tag_name"), row.get("score")))
        .collect())
}
