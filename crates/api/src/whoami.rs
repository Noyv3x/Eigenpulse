use axum::{extract::State, Extension, Json};
use ep_auth::AuthPat;
use ep_core::AppState;
use serde::Serialize;

use crate::errors::ApiError;

#[derive(Serialize)]
pub struct WhoamiResp {
    pub user: UserBrief,
    pub token: TokenBrief,
}
#[derive(Serialize)]
pub struct UserBrief { pub handle: String, pub name: String, pub role: String }
#[derive(Serialize)]
pub struct TokenBrief { pub name: String, pub scopes: Vec<String> }

pub async fn handler(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<WhoamiResp>, ApiError> {
    let row: (String, String, String) = sqlx::query_as(
        "SELECT handle, name, role FROM app_user WHERE id = 1"
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Json(WhoamiResp {
        user: UserBrief { handle: row.0, name: row.1, role: row.2 },
        token: TokenBrief { name: pat.name.clone(), scopes: pat.scopes.clone() },
    }))
}
