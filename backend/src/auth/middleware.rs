use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::Utc;

use crate::error::AppError;
use crate::state::AppState;

pub const COOKIE_NAME: &str = "efflux_session";

/// 守在 API 前面的门：没设置访问口令时整扇门敞开（只适合本机开发）。
pub async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !state.access.is_protected() {
        return next.run(request).await;
    }

    let authorized = request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(read_session)
        .map(|token| state.access.verify_token(&token, Utc::now().timestamp()))
        .unwrap_or(false);

    if authorized {
        return next.run(request).await;
    }

    AppError::Unauthorized("需要先输入访问口令".to_string()).into_response()
}

fn read_session(cookies: &str) -> Option<String> {
    cookies.split(';').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        (name.trim() == COOKIE_NAME).then(|| value.trim().to_string())
    })
}
