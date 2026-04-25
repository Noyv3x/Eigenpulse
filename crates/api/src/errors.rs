use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiErrorBody { pub error: ApiErrorInner }
#[derive(Debug, Serialize)]
pub struct ApiErrorInner { pub code: &'static str, pub message: String }

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("bad request: {0}")] BadRequest(String),
    #[error("unauthorized")] Unauthorized,
    #[error("forbidden: {0}")] Forbidden(String),
    #[error("not found")] NotFound,
    #[error("internal: {0}")] Internal(String),
}

impl ApiError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::NotFound => "not_found",
            Self::Internal(_) => "internal",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = ApiErrorBody {
            error: ApiErrorInner { code: self.code(), message: self.to_string() }
        };
        (status, axum::Json(body)).into_response()
    }
}

impl From<sqlx::Error> for ApiError { fn from(e: sqlx::Error) -> Self { Self::Internal(e.to_string()) } }
impl From<anyhow::Error> for ApiError { fn from(e: anyhow::Error) -> Self { Self::Internal(e.to_string()) } }
