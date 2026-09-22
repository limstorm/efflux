use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// 领域错误层级：可预期的操作性错误 → 结构化响应；程序性错误 → 记录日志 + 通用 500。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{resource} 不存在: {id}")]
    NotFound {
        resource: &'static str,
        id: String,
    },

    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    PayloadTooLarge(String),

    #[error("{0}")]
    Unsupported(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorPayload,
}

#[derive(Serialize)]
struct ErrorPayload {
    code: &'static str,
    message: String,
}

impl AppError {
    pub fn not_found(resource: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound {
            resource,
            id: id.into(),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::NotFound { .. } => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            AppError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR"),
            AppError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            AppError::PayloadTooLarge(_) => (StatusCode::PAYLOAD_TOO_LARGE, "PAYLOAD_TOO_LARGE"),
            AppError::Unsupported(_) => (StatusCode::UNSUPPORTED_MEDIA_TYPE, "UNSUPPORTED_MEDIA"),
            AppError::Database(sqlx::Error::RowNotFound) => {
                (StatusCode::NOT_FOUND, "NOT_FOUND")
            }
            AppError::Database(_) | AppError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR")
            }
        }
    }

    /// 是否可以直接把消息展示给用户（操作类错误可以，程序错误不行）
    fn is_operational(&self) -> bool {
        !matches!(self, AppError::Database(_) | AppError::Internal(_))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();

        if !self.is_operational() {
            tracing::error!(error = %self, "未预期的服务端错误");
        } else if status.is_server_error() {
            tracing::warn!(error = %self, "服务端错误");
        }

        let message = if self.is_operational() {
            self.to_string()
        } else {
            "服务出了点小状况，请稍后再试".to_string()
        };

        (
            status,
            Json(ErrorBody {
                error: ErrorPayload { code, message },
            }),
        )
            .into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
