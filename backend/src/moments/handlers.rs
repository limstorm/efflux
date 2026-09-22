use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use uuid::Uuid;

use super::models::{ListQuery, MomentDetail, MomentInput, MomentListResponse, StatsResponse};
use super::service;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<MomentListResponse>> {
    let response = service::list(&state.pool, query).await?;
    Ok(Json(response))
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<MomentInput>,
) -> AppResult<(StatusCode, Json<MomentDetail>)> {
    let detail = service::create(&state.pool, input).await?;
    Ok((StatusCode::CREATED, Json(detail)))
}

pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<MomentDetail>> {
    let detail = service::detail(&state.pool, id).await?;
    Ok(Json(detail))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<MomentInput>,
) -> AppResult<Json<MomentDetail>> {
    let detail = service::update(&state.pool, id, input).await?;
    Ok(Json(detail))
}

pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::delete(&state.pool, &state.storage, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn stats(State(state): State<AppState>) -> AppResult<Json<StatsResponse>> {
    let stats = service::stats(&state.pool).await?;
    Ok(Json(stats))
}

// ---------------------------------------------------------------- 分享

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareResponse {
    pub token: String,
}

/// 拿到这条记忆的分享凭证（已有就沿用）
pub async fn share(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ShareResponse>> {
    let token = service::share(&state.pool, id).await?;
    Ok(Json(ShareResponse { token }))
}

/// 收回分享
pub async fn unshare(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::unshare(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 分享页读的只读内容。挂在口令保护之外——凭证本身就是钥匙
pub async fn shared(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> AppResult<Json<MomentDetail>> {
    Ok(Json(service::shared(&state.pool, &token).await?))
}

#[derive(Debug, Deserialize)]
pub struct SharedMediaQuery {
    /// 要缩略图而不是原文件
    pub thumb: Option<String>,
}

/// 分享页里的画面与音乐走这里，而不是 /files/。
///
/// 生产环境 /files/ 被 Nginx 的 auth_request 挡着，外人打开分享链接只会
/// 看到裂图；而这里的凭证是有效的，校验通过就该放行。
pub async fn shared_media(
    State(state): State<AppState>,
    Path((token, media_id)): Path<(String, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<SharedMediaQuery>,
) -> AppResult<Response> {
    let (path, mime) = service::shared_media(
        &state.pool,
        &state.storage,
        &token,
        media_id,
        query.thumb.is_some(),
    )
    .await?;

    let total = tokio::fs::metadata(&path)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .len();

    // 视频要能边下边播、要能拖进度，靠的全是 Range：少了它，浏览器拿不到
    // 总长度、请求又被整份回过去，就会一直停在"正在加载"。
    // 认不出的 Range 一律回全量——宁可慢一点，也不要给一个错的片段。
    let range = parse_range(headers.get(header::RANGE), total);
    let mut file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let (status, start, end) = match range {
        Some((start, end)) => (StatusCode::PARTIAL_CONTENT, start, end),
        None => (StatusCode::OK, 0, total.saturating_sub(1)),
    };
    let length = if total == 0 { 0 } else { end - start + 1 };
    if start > 0 {
        file.seek(SeekFrom::Start(start))
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, length)
        // 有了它，浏览器才知道"这个地址可以按段取"
        .header(header::ACCEPT_RANGES, "bytes")
        // 链接随时可能被收回，别让中间层缓存太久
        .header(header::CACHE_CONTROL, "private, max-age=600");
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{total}"),
        );
    }

    builder
        .body(axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(
            file.take(length),
        )))
        .map_err(|e| AppError::Internal(e.into()))
}

/// 解析 `Range: bytes=…`，只认单段（视频一次只取一段）。
///
/// 支持 `bytes=N-M`、`bytes=N-`、`bytes=-N` 三种写法；越界或残缺就返回 None，
/// 由调用方回整份——规范允许服务端忽略 Range，这比猜一个片段安全。
fn parse_range(header: Option<&HeaderValue>, total: u64) -> Option<(u64, u64)> {
    let raw = header?.to_str().ok()?;
    let spec = raw.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (left, right) = spec.split_once('-')?;
    let (left, right) = (left.trim(), right.trim());
    let (start, end) = if left.is_empty() {
        let suffix: u64 = right.parse().ok()?;
        (total.saturating_sub(suffix), total.saturating_sub(1))
    } else if right.is_empty() {
        (left.parse().ok()?, total.saturating_sub(1))
    } else {
        (left.parse().ok()?, right.parse().ok()?)
    };

    if total == 0 || start > end || start >= total {
        return None;
    }
    Some((start, end.min(total.saturating_sub(1))))
}

// ---------------------------------------------------------------- 定位辅助

#[derive(Debug, Deserialize)]
pub struct CityQuery {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct CityResponse {
    pub city: Option<String>,
}

/// 定位之后立刻问一句「这是哪个市」，好让表单里当场看到城市，
/// 而不是保存完才发现归到了别处。附近没有表内城市就返回 null，由人自己填。
pub async fn city_at(Query(query): Query<CityQuery>) -> AppResult<Json<CityResponse>> {
    let city = match (query.latitude, query.longitude) {
        (Some(latitude), Some(longitude)) => {
            crate::geo::nearest_city(latitude, longitude).map(|name| name.to_string())
        }
        _ => None,
    };
    Ok(Json(CityResponse { city }))
}
