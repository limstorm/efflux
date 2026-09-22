use std::collections::HashMap;

use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::models::{
    ListQuery, MediaDto, MediaRow, MomentDetail, MomentInput, MomentListResponse, MomentRow,
    MomentSummary, Place, StatsResponse,
};
use super::repo::{self, MomentFilter};
use crate::error::{AppError, AppResult};

const MAX_TITLE_CHARS: usize = 120;
const MAX_CONTENT_CHARS: usize = 20_000;
const MAX_TAGS: usize = 12;
const MAX_TAG_CHARS: usize = 32;
const MAX_PLACE_CHARS: usize = 60;
const MAX_CITY_CHARS: usize = 24;
/// 氛围音只有这四种，名字两边写死：前端 frontend/src/audio/ambient.ts 的 id
const AMBIENT_NAMES: [&str; 4] = ["rain", "night", "fire", "pad"];
const EXCERPT_CHARS: usize = 80;
const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 1000;

pub async fn create(pool: &PgPool, input: MomentInput) -> AppResult<MomentDetail> {
    let title = normalize_title(input.title.as_deref().unwrap_or_default())?;
    let content = normalize_content(input.content.as_deref().unwrap_or_default())?;
    let tags = normalize_tags(input.tags.clone().unwrap_or_default());
    let mut media_links = input.media.clone().unwrap_or_default();
    media_links.sort_by_key(|link| link.position.unwrap_or(0));
    let media_ids: Vec<Uuid> = media_links.into_iter().map(|link| link.id).collect();

    if title.is_empty() && content.is_empty() && media_ids.is_empty() {
        return Err(AppError::validation("写点什么吧，一句话或一张照片都可以"));
    }

    let happened_at = validate_time(input.happened_at)?;
    let place = normalize_place(
        input.location_name.as_deref(),
        input.city.as_deref(),
        input.latitude,
        input.longitude,
    )?;

    // 空串是「清掉」的信号，只有更新时用得上；新建时不选氛围就存 NULL
    let ambient = normalize_ambient(input.ambient.as_deref())?.filter(|name| !name.is_empty());

    let mut tx = pool.begin().await?;
    let row = repo::insert(
        &mut tx,
        &title,
        &content,
        happened_at,
        &place,
        ambient.as_deref(),
    )
    .await?;

    if !tags.is_empty() {
        let tag_ids = repo::upsert_tags(&mut tx, &tags).await?;
        repo::link_tags(&mut tx, row.id, &tag_ids).await?;
    }

    if !media_ids.is_empty() {
        let attached = repo::attach_media(&mut tx, row.id, &media_ids).await?;
        if (attached as usize) < media_ids.len() {
            tracing::warn!(
                moment_id = %row.id,
                requested = media_ids.len(),
                attached,
                "部分媒体未能关联（可能不存在或已被其他点滴占用）"
            );
        }
    }

    tx.commit().await?;

    detail(pool, row.id).await
}

pub async fn update(pool: &PgPool, id: Uuid, input: MomentInput) -> AppResult<MomentDetail> {
    let existing = repo::find(pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("点滴", id.to_string()))?;

    let title = match input.title {
        Some(ref t) => Some(normalize_title(t)?),
        None => None,
    };
    let content = match input.content {
        Some(ref c) => Some(normalize_content(c)?),
        None => None,
    };
    let happened_at = match input.happened_at {
        Some(t) => Some(validate_time(Some(t))?),
        None => None,
    };
    // 地点这几个字段都没传就完全不动；传了任一个（哪怕只是改成空）就整体替换
    let place = match (
        input.location_name.as_deref(),
        input.city.as_deref(),
        input.latitude,
        input.longitude,
    ) {
        (None, None, None, None) => None,
        (name, city, latitude, longitude) => {
            Some(normalize_place(name, city, latitude, longitude)?)
        }
    };
    // 不给就不动；给空串 = 不要了（落库时换成 NULL）；给别的名字必须认识
    let ambient = normalize_ambient(input.ambient.as_deref())?;

    // 更新后仍须至少保留一项内容
    let next_title = title.as_deref().unwrap_or(&existing.title);
    let next_content = content.as_deref().unwrap_or(&existing.content);
    if next_title.is_empty() && next_content.is_empty() && input.media.is_none() {
        return Err(AppError::validation("点滴不能完全空着"));
    }

    let mut tx = pool.begin().await?;
    repo::update(
        &mut tx,
        id,
        title.as_deref(),
        content.as_deref(),
        happened_at,
        place.as_ref(),
        ambient.as_deref(),
    )
    .await?
    .ok_or_else(|| AppError::not_found("点滴", id.to_string()))?;

    if let Some(tags) = input.tags {
        let tags = normalize_tags(tags);
        repo::clear_tags(&mut tx, id).await?;
        if !tags.is_empty() {
            let tag_ids = repo::upsert_tags(&mut tx, &tags).await?;
            repo::link_tags(&mut tx, id, &tag_ids).await?;
        }
    }

    if let Some(media) = input.media {
        let media_ids: Vec<Uuid> = media.into_iter().map(|m| m.id).collect();
        repo::detach_media(&mut tx, id).await?;
        if !media_ids.is_empty() {
            repo::attach_media(&mut tx, id, &media_ids).await?;
        }
    }

    tx.commit().await?;

    detail(pool, id).await
}

pub async fn detail(pool: &PgPool, id: Uuid) -> AppResult<MomentDetail> {
    let row = repo::find(pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("点滴", id.to_string()))?;
    let media = repo::media_for_moment(pool, id).await?;
    let tags = repo::tags_for_moment(pool, id).await?;
    Ok(build_detail(row, media, tags))
}

// ---------------------------------------------------------------- 分享

/// 拿到这条记忆的分享凭证。已经有就沿用，没有才新造一个。
///
/// 用 128 位密码学随机（32 个 hex 字符），不用自增 id、也不用时间戳——
/// 那种值能猜、能枚举，等于把整库的记忆挂在网上任人翻。
pub async fn share(pool: &PgPool, id: Uuid) -> AppResult<String> {
    let existing = repo::find(pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("点滴", id.to_string()))?;

    // 分享过就沿用原值，别让已经发出去的链接失效
    if let Some(token) = existing.share_token {
        return Ok(token);
    }

    let token = Uuid::new_v4().simple().to_string();
    repo::set_share_token(pool, id, &token)
        .await?
        .ok_or_else(|| AppError::not_found("点滴", id.to_string()))
}

/// 收回分享。旧链接立刻失效，下次再分享会拿到一个新值。
pub async fn unshare(pool: &PgPool, id: Uuid) -> AppResult<()> {
    if !repo::clear_share_token(pool, id).await? {
        return Err(AppError::not_found("点滴", id.to_string()));
    }
    Ok(())
}

/// 分享页读的那一份：只认凭证，不看登录状态。
/// 凭证本身就是钥匙，所以这里不额外要求口令。
pub async fn shared(pool: &PgPool, token: &str) -> AppResult<MomentDetail> {
    let row = repo::find_by_share_token(pool, token)
        .await?
        .ok_or_else(|| AppError::not_found("分享", "这个链接已经失效了".to_string()))?;
    let media = repo::media_for_moment(pool, row.id).await?;
    let tags = repo::tags_for_moment(pool, row.id).await?;
    let mut detail = build_detail(row, media, tags);

    // 画面与音乐改走凭证路由。生产环境 /files/ 被 Nginx 的 auth_request 挡着，
    // 外人打开分享链接只会看到裂图；而这里的凭证是有效的，就该放行。
    for item in &mut detail.media {
        item.url = shared_media_url(token, item.id, false);
        if item.thumb_url.is_some() {
            item.thumb_url = Some(shared_media_url(token, item.id, true));
        }
    }
    if let Some(music) = detail.music.as_mut() {
        music.url = shared_media_url(token, music.id, false);
    }

    Ok(detail)
}

/// 分享页里一份媒体的地址：带上凭证，绕开 /files/ 的鉴权
fn shared_media_url(token: &str, media_id: Uuid, thumb: bool) -> String {
    let base = format!("/api/shared/{token}/media/{media_id}");
    if thumb {
        format!("{base}?thumb=1")
    } else {
        base
    }
}

/// 分享页要的一份媒体：给出磁盘路径与类型，交给 handler 流式吐出去。
///
/// 校验两件事——凭证还在，且这份媒体确实属于这条记忆。
/// 不能拿 A 的凭证去读 B 的照片。
pub async fn shared_media(
    pool: &PgPool,
    storage: &crate::media::Storage,
    token: &str,
    media_id: Uuid,
    thumb: bool,
) -> AppResult<(std::path::PathBuf, String)> {
    let row = repo::find_by_share_token(pool, token)
        .await?
        .ok_or_else(|| AppError::not_found("分享", "这个链接已经失效了".to_string()))?;

    let media = repo::media_for_moment(pool, row.id).await?;
    let item = media
        .iter()
        .find(|one| one.id == media_id)
        .ok_or_else(|| AppError::not_found("画面", media_id.to_string()))?;

    // 没有缩略图就退回原文件，别让视频封面裂掉
    let rel = if thumb {
        item.thumb_path.clone().unwrap_or_else(|| item.file_path.clone())
    } else {
        item.file_path.clone()
    };

    Ok((storage.root().join(rel), item.mime_type.clone()))
}

pub async fn delete(pool: &PgPool, storage: &crate::media::Storage, id: Uuid) -> AppResult<()> {
    // 先把媒体路径取出来，删除记录后再清理磁盘，避免留下无主的文件
    let files = repo::files_for_moment(pool, id).await?;

    let removed = repo::delete(pool, id).await?;
    if !removed {
        return Err(AppError::not_found("点滴", id.to_string()));
    }

    for (file, thumb) in files {
        storage.remove(&file).await;
        if let Some(thumb) = thumb {
            storage.remove(&thumb).await;
        }
    }

    Ok(())
}

pub async fn list(pool: &PgPool, query: ListQuery) -> AppResult<MomentListResponse> {
    let filter = normalize_filter(query);
    let (rows, total) = repo::list(pool, &filter).await?;

    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let media = repo::media_for_moments(pool, &ids).await?;
    let tag_pairs = repo::tags_for_moments(pool, &ids).await?;

    let mut media_by_moment: HashMap<Uuid, Vec<MediaRow>> = HashMap::new();
    for item in media {
        if let Some(moment_id) = item.moment_id {
            media_by_moment.entry(moment_id).or_default().push(item);
        }
    }
    let mut tags_by_moment: HashMap<Uuid, Vec<String>> = HashMap::new();
    for (moment_id, name) in tag_pairs {
        tags_by_moment.entry(moment_id).or_default().push(name);
    }

    let items = rows
        .into_iter()
        .map(|row| {
            let media = media_by_moment.remove(&row.id).unwrap_or_default();
            let tags = tags_by_moment.remove(&row.id).unwrap_or_default();
            build_summary(row, media, tags)
        })
        .collect();

    Ok(MomentListResponse { items, total })
}

pub async fn stats(pool: &PgPool) -> AppResult<StatsResponse> {
    let (total, first, last, tag_count) = repo::stats(pool).await?;
    Ok(StatsResponse {
        total,
        first_happened_at: first,
        last_happened_at: last,
        tag_count,
    })
}

// ---------------------------------------------------------------- 组装

fn build_summary(row: MomentRow, media: Vec<MediaRow>, tags: Vec<String>) -> MomentSummary {
    let has_music = media.iter().any(|m| m.kind == "audio");
    let visual: Vec<&MediaRow> = media.iter().filter(|m| m.kind != "audio").collect();
    let cover = visual
        .iter()
        .find(|m| m.kind == "image")
        .or_else(|| visual.first())
        .map(|m| MediaDto::from((*m).clone()));

    // 先把地点取出来：下面 row 的字段会被逐个移走
    let place = Place::from_row(&row);

    MomentSummary {
        excerpt: make_excerpt(&row.content),
        id: row.id,
        title: row.title,
        happened_at: row.happened_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
        tags,
        cover,
        media_count: visual.len(),
        has_music,
        place,
    }
}

fn build_detail(row: MomentRow, media: Vec<MediaRow>, tags: Vec<String>) -> MomentDetail {
    let mut visual = Vec::new();
    let mut music = None;
    for item in media {
        if item.kind == "audio" {
            if music.is_none() {
                music = Some(MediaDto::from(item));
            }
        } else {
            visual.push(MediaDto::from(item));
        }
    }
    let place = Place::from_row(&row);
    let share_token = row.share_token.clone();
    let ambient = row.ambient.clone();

    MomentDetail {
        id: row.id,
        title: row.title,
        content: row.content,
        happened_at: row.happened_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
        tags,
        media: visual,
        music,
        ambient,
        place,
        share_token,
    }
}

fn make_excerpt(content: &str) -> String {
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= EXCERPT_CHARS {
        return flat;
    }
    let sliced: String = flat.chars().take(EXCERPT_CHARS).collect();
    format!("{sliced}…")
}

// ---------------------------------------------------------------- 校验 / 规范化

fn normalize_title(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.chars().count() > MAX_TITLE_CHARS {
        return Err(AppError::validation(format!(
            "主题太长了，最多 {MAX_TITLE_CHARS} 个字"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_content(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.chars().count() > MAX_CONTENT_CHARS {
        return Err(AppError::validation(format!(
            "文字太长了，最多 {MAX_CONTENT_CHARS} 个字"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_tags(raw: Vec<String>) -> Vec<String> {
    let mut seen = Vec::new();
    for tag in raw {
        let name = tag.trim().chars().take(MAX_TAG_CHARS).collect::<String>();
        if name.is_empty() {
            continue;
        }
        if !seen.iter().any(|existing: &String| existing.eq_ignore_ascii_case(&name)) {
            seen.push(name);
        }
        if seen.len() >= MAX_TAGS {
            break;
        }
    }
    seen
}

/// 氛围音：现场合成的四种，没有文件可存，存的只是名字。
///
/// 空串是「不要了」的信号，落库前由 SQL 的 NULLIF 换成 NULL；
/// 不认识的名字直接拒绝——将来前端改了名字，老数据也不会变成一串谁也放不出来的字符串。
fn normalize_ambient(raw: Option<&str>) -> AppResult<Option<String>> {
    let Some(value) = raw else {
        return Ok(None);
    };
    let name = value.trim().to_ascii_lowercase();
    if name.is_empty() {
        return Ok(Some(String::new()));
    }
    if !AMBIENT_NAMES.contains(&name.as_str()) {
        return Err(AppError::validation("没有这种氛围音"));
    }
    Ok(Some(name))
}

fn validate_time(value: Option<chrono::DateTime<Utc>>) -> AppResult<chrono::DateTime<Utc>> {
    match value {
        None => Ok(Utc::now()),
        Some(t) => {
            let lower = chrono::DateTime::from_timestamp(0, 0).unwrap();
            let upper = Utc::now() + Duration::days(365 * 20);
            if t < lower || t > upper {
                return Err(AppError::validation("时间似乎不太对，请重新选择"));
            }
            Ok(t)
        }
    }
}

/// 地点一整套：名字与城市限长，坐标限范围，只给一半坐标就都不留（定位不了）。
///
/// 城市没手填但有坐标时，顺手推断最近的市——地图上那一盏灯得靠城市才点得起来。
fn normalize_place(
    name: Option<&str>,
    city: Option<&str>,
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> AppResult<repo::PlaceInput> {
    let name = name.unwrap_or_default().trim();
    if name.chars().count() > MAX_PLACE_CHARS {
        return Err(AppError::validation(format!(
            "地点太长了，最多 {MAX_PLACE_CHARS} 个字"
        )));
    }

    let mut city = city.unwrap_or_default().trim().to_string();
    if city.chars().count() > MAX_CITY_CHARS {
        return Err(AppError::validation(format!(
            "城市名太长了，最多 {MAX_CITY_CHARS} 个字"
        )));
    }

    let latitude = latitude.filter(|v| v.is_finite() && (-90.0..=90.0).contains(v));
    let longitude = longitude.filter(|v| v.is_finite() && (-180.0..=180.0).contains(v));
    let (latitude, longitude) = match (latitude, longitude) {
        (Some(lat), Some(lon)) => (Some(lat), Some(lon)),
        _ => (None, None),
    };

    if city.is_empty() {
        if let (Some(lat), Some(lon)) = (latitude, longitude) {
            city = crate::geo::nearest_city(lat, lon).unwrap_or_default().to_string();
        }
    }

    Ok(repo::PlaceInput {
        name: name.to_string(),
        city,
        latitude,
        longitude,
    })
}

fn normalize_filter(query: ListQuery) -> MomentFilter {
    let q = query
        .q
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| repo::escape_like(&s));

    let tags = query.tags.map(|raw| {
        raw.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    })
    .filter(|v| !v.is_empty());

    let ascending = query
        .order
        .as_deref()
        .map(|o| o.eq_ignore_ascii_case("asc"))
        .unwrap_or(false);

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);

    MomentFilter {
        q,
        tags,
        from: query.from,
        to: query.to,
        ascending,
        limit,
        offset,
    }
}
