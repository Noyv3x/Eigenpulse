use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ApiError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl ApiError {
    pub(crate) fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::Forbidden(_) => "forbidden",
            Self::Internal(_) => "internal",
        }
    }

    fn client_message(&self) -> String {
        match self {
            Self::BadRequest(message) | Self::Forbidden(message) => message.clone(),
            Self::Internal(e) => {
                tracing::warn!(error = %e, "open api internal error");
                "internal server error".into()
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let code = self.code();
        let message = self.client_message();
        ep_core::api_error_response(status, code, message)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::ApiError;

    #[test]
    fn internal_error_client_message_is_generic() {
        let err = ApiError::Internal("sqlite://secret/path failed".into());
        assert_eq!(err.client_message(), "internal server error");
    }

    #[test]
    fn user_errors_keep_actionable_message() {
        let err = ApiError::BadRequest("title is required".into());
        assert_eq!(err.client_message(), "title is required");
    }

    #[test]
    fn forbidden_message_does_not_repeat_error_code() {
        let err = ApiError::Forbidden("requires scope: notify:write".into());
        assert_eq!(err.client_message(), "requires scope: notify:write");
    }
}
