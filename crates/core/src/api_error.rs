use axum::async_trait;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Query, Request};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub error: ApiErrorInner,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorInner {
    pub code: String,
    pub message: String,
}

pub fn api_error_response(
    status: StatusCode,
    code: impl Into<String>,
    message: impl Into<String>,
) -> Response {
    (
        status,
        Json(ApiErrorBody {
            error: ApiErrorInner {
                code: code.into(),
                message: message.into(),
            },
        }),
    )
        .into_response()
}

#[derive(Debug)]
pub struct ApiJson<T>(pub T);

#[derive(Debug)]
pub struct ApiQuery<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(json_rejection_response)?;
        Ok(Self(value))
    }
}

fn json_rejection_response(rejection: JsonRejection) -> Response {
    api_error_response(rejection.status(), "bad_request", rejection.body_text())
}

#[async_trait]
impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(query_rejection_response)?;
        Ok(Self(value))
    }
}

fn query_rejection_response(rejection: QueryRejection) -> Response {
    api_error_response(rejection.status(), "bad_request", rejection.body_text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Request};

    async fn assert_json_error_response(response: Response, status: StatusCode, code: &str) {
        assert_eq!(response.status(), status);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.starts_with("application/json")),
            Some(true)
        );
        let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(
            json.pointer("/error/code").and_then(|v| v.as_str()),
            Some(code)
        );
        assert!(
            json.pointer("/error/message")
                .and_then(|v| v.as_str())
                .is_some_and(|msg| !msg.is_empty()),
            "missing message: {json}"
        );
    }

    #[test]
    fn api_error_body_shape_is_stable() {
        let body = ApiErrorBody {
            error: ApiErrorInner {
                code: "bad_request".into(),
                message: "invalid input".into(),
            },
        };

        let json = serde_json::to_value(body).expect("serialize");

        assert_eq!(
            json,
            serde_json::json!({
                "error": {
                    "code": "bad_request",
                    "message": "invalid input"
                }
            })
        );
    }

    #[tokio::test]
    async fn api_json_rejection_uses_shared_error_shape() {
        let req = Request::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{"))
            .expect("request");
        let response = ApiJson::<serde_json::Value>::from_request(req, &())
            .await
            .expect_err("malformed json should fail");

        assert_json_error_response(response, StatusCode::BAD_REQUEST, "bad_request").await;
    }

    #[tokio::test]
    async fn api_query_rejection_uses_shared_error_shape() {
        #[derive(Debug, serde::Deserialize)]
        struct Input {
            #[serde(rename = "period")]
            _period: String,
        }

        let mut parts = Request::builder()
            .uri("/budget")
            .body(Body::empty())
            .expect("request")
            .into_parts()
            .0;
        let response = ApiQuery::<Input>::from_request_parts(&mut parts, &())
            .await
            .expect_err("missing query field should fail");

        assert_json_error_response(response, StatusCode::BAD_REQUEST, "bad_request").await;
    }
}
