use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use std::path::Path;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::auth;
use crate::error::AppResult;
use crate::backup;
use crate::gallery;
use crate::map;
use crate::moments;
use crate::ratelimit;
use crate::state::AppState;
use crate::tags;

pub fn build(state: AppState) -> Router {
    // 需要口令才能访问的部分（健康检查与登录接口不在此列）
    let protected = Router::new()
        .route("/moments", get(moments::list).post(moments::create))
        .route("/moments/stats", get(moments::stats))
        .route("/geo/city", get(moments::city_at))
        .route("/backup", get(backup::api_list).post(backup::api_create))
        .route(
            "/backup/settings",
            get(backup::api_settings).patch(backup::api_set_settings),
        )
        .route(
            "/backup/restore",
            post(backup::api_restore)
                // 一份备份可能比单个附件大得多，这里单独放宽
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024 * 1024)),
        )
        .route("/backup/{filename}", get(backup::api_download))
        .route("/gallery/{id}/similar", get(gallery::similar))
        .route(
            "/moments/{id}",
            get(moments::detail)
                .patch(moments::update)
                .delete(moments::remove),
        )
        .route(
            "/moments/{id}/share",
            post(moments::share).delete(moments::unshare),
        )
        // 视频不限大小，所以这条路由要把全局那道 body 上限摘掉。
        // 路由自带的层比外层更靠内、最后写进去，于是它说了算；
        // 图片与音频的限额改由 Storage 流式守着（见 media/storage.rs）。
        .route(
            "/media/upload/{kind}",
            post(crate::media::upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/media/{id}", axum::routing::delete(crate::media::remove))
        .route("/tags", get(tags::list))
        .route("/gallery", get(gallery::list))
        .route("/gallery/tags", get(gallery::tags))
        .route("/map/provinces", get(map::provinces))
        .route("/map/cities", get(map::cities))
        // 给 Nginx 的 auth_request 用：能让它跑通就说明带着有效的会话
        .route("/auth/check", get(auth::check))
        .route("/auth/session", get(auth::session))
        .route("/auth/change-code", post(auth::change_code))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    let api = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/auth/status", get(auth::status))
        .route(
            "/auth/login",
            // 登录单独一只更紧的桶，不再被下面那只 API 桶重复扣
            post(auth::login).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                ratelimit::guard_login,
            )),
        )
        .route("/auth/logout", post(auth::logout))
        // 分享出去的链接要给没登录的人看：凭证本身就是钥匙。
        // 媒体也走这里，绕开 /files/ 上那道鉴权
        .route("/shared/{token}", get(moments::shared))
        .route("/shared/{token}/media/{id}", get(moments::shared_media))
        .merge(protected)
        // 没对上的 /api/xxx 仍旧回 JSON：别让它落到前端回退里去
        .fallback(not_found)
        // 限流：本地部署没有 Nginx，这一层就由后端自己站
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            ratelimit::guard_api,
        ))
        .with_state(state.clone());

    // 媒体文件：由后端直出（Docker 部署里 Nginx 直接接管 /files，不走这里）。
    // 门和 API 是同一道：设了口令之后，未登录连照片 URL 都取不到。
    let files = ServeDir::new(state.storage.root().to_path_buf())
        .append_index_html_on_directories(false);
    let files = ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .service(files);
    let files_router = Router::new().nest_service("/files", files);

    let app = Router::new()
        .nest("/api", api)
        .merge(files_router)
        // 全局兜底：JSON 之类的请求仍旧有上限，免得被一个超大的 body 拖垮。
        // 媒体上传那条路由自己把这道上限摘了（视频不限大小，见上）
        .layer(DefaultBodyLimit::max(
            state.config.max_upload_bytes + 8 * 1024 * 1024,
        ))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors(&state.config.cors_origins));

    // 单机部署（EFFLUX_STATIC_DIR）：后端顺手把打包好的前端也发出去。
    // Docker 部署里这份活是 Nginx 干的，这里不启用。
    match state.config.static_dir.clone() {
        Some(dir) => app.fallback_service(spa_service(&dir)),
        None => app.fallback(not_found),
    }
}

/// 前端静态文件：存在的照发，找不到的交给 index.html，
/// 剩下的路由由前端自己接（单页应用回退）。状态码不动，深链接才是 200。
///
/// 外面裹一层「HTML 不缓存」：带 hash 的资源可以放心留，入口那份不行 ——
/// 不然升级之后浏览器还捧着旧页面，得手动刷一次才看得见新版本。
fn spa_service(dir: &Path) -> Router {
    let spa = ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")));
    let spa = ServiceBuilder::new()
        .layer(axum::middleware::from_fn(fresh_html))
        .service(spa);
    Router::new().fallback_service(spa)
}

/// /assets/ 下面都是带内容指纹的文件（JS、CSS、字体分片），可以长留；
/// 其余（index.html、favicon 这些短名字）让浏览器每次回来问一句。
///
/// 之前这里只写了「其余 no-cache」，**没给 /assets/ 设过缓存头**——于是字体
/// 每次都重新下载，进记忆页就每次都「换一次字体、整页重排、闪一下」
/// （用户报的正是这个）。带指纹的文件改内容必改名，所以 immutable 是安全的。
async fn fresh_html(request: Request, next: Next) -> Response {
    let hashed_asset = request.uri().path().starts_with("/assets/");
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        if hashed_asset {
            HeaderValue::from_static("public, max-age=31536000, immutable")
        } else {
            HeaderValue::from_static("no-cache")
        },
    );
    response
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn ready(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    sqlx::query("SELECT 1").execute(&state.pool).await?;
    Ok(Json(json!({ "status": "ok", "database": "ok" })))
}

async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": { "code": "NOT_FOUND", "message": "没有找到这个地址" }
        })),
    )
}

fn cors(origins: &[String]) -> CorsLayer {
    let allowed: Vec<HeaderValue> = origins.iter().filter_map(|o| o.parse().ok()).collect();
    CorsLayer::new()
        .allow_origin(allowed)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .max_age(Duration::from_secs(3600))
}
