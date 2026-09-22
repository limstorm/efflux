use std::time::Duration;

use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use super::middleware::COOKIE_NAME;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    pub code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeCodeInput {
    pub current_code: String,
    pub new_code: String,
}

/// `POST /api/auth/login` — 用家庭口令换一个会话 Cookie
pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginInput>,
) -> AppResult<Response> {
    if !state.access.is_protected() {
        // 未设置口令的部署（本机开发）不需要登录
        return Ok(Json(json!({ "ok": true, "required": false })).into_response());
    }

    if !state.access.verify_code(input.code.trim()) {
        // 稍微拖一下，给暴力尝试减速
        tokio::time::sleep(Duration::from_millis(500)).await;
        tracing::warn!("口令尝试失败");
        return Err(AppError::Unauthorized("口令不对，再想想".to_string()));
    }

    let Some(token) = state.access.issue_token(Utc::now().timestamp()) else {
        return Err(AppError::Unauthorized("当前部署未设置口令".to_string()));
    };

    tracing::info!("新的会话已建立");
    Ok(with_session_cookie(&state, &token))
}

/// `GET /api/auth/status` — 公开接口：只回答「这个部署要不要口令」
pub async fn status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({ "protected": state.access.is_protected() }))
}

/// `GET /api/auth/session` — 当前会话的概览（能走到这里就说明已登录）
pub async fn session(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "protected": state.access.is_protected(),
        "sessionDays": state.config.session_days,
    }))
}

/// `POST /api/auth/change-code` — 改口令，立即生效，不用重启
pub async fn change_code(
    State(state): State<AppState>,
    Json(input): Json<ChangeCodeInput>,
) -> AppResult<Response> {
    if !state.access.is_protected() {
        return Err(AppError::Validation(
            "当前部署还没有设置访问口令".to_string(),
        ));
    }

    let current = input.current_code.trim();
    let next = input.new_code.trim();

    if !state.access.verify_code(current) {
        tokio::time::sleep(Duration::from_millis(500)).await;
        tracing::warn!("修改口令时旧口令校验失败");
        return Err(AppError::Unauthorized("当前口令不对".to_string()));
    }
    if next.chars().count() < 6 {
        return Err(AppError::Validation("新口令至少 6 位".to_string()));
    }
    if next == current {
        return Err(AppError::Validation("新口令和现在的一样".to_string()));
    }

    state.access.change_code(&state.pool, next).await?;

    // 改口令会踢掉所有旧会话，这里给操作者本人补一张新通行证
    let Some(token) = state.access.issue_token(Utc::now().timestamp()) else {
        return Err(AppError::Internal(anyhow::anyhow!("口令刚写入却读不到")));
    };
    Ok(with_session_cookie(&state, &token))
}

/// `GET /api/auth/check` — 供 Nginx 的 auth_request 调用
pub async fn check() -> StatusCode {
    StatusCode::NO_CONTENT
}

/// `POST /api/auth/logout` — 忘掉这次会话
pub async fn logout() -> Response {
    let cookie = format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    let mut response = Json(json!({ "ok": true })).into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

fn with_session_cookie(state: &AppState, token: &str) -> Response {
    let cookie = build_cookie(
        token,
        state.access.ttl_secs(),
        state.config.cookie_secure,
    );

    let mut response = Json(json!({ "ok": true })).into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    response
}

fn build_cookie(token: &str, max_age: i64, secure: bool) -> String {
    let mut cookie =
        format!("{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}
