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
pub struct UserBrief {
    pub handle: String,
    pub name: String,
    pub role: String,
}
#[derive(Serialize)]
pub struct TokenBrief {
    pub name: String,
    pub scopes: Vec<String>,
}

pub async fn handler(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<WhoamiResp>, ApiError> {
    let row: (String, String, String) =
        sqlx::query_as("SELECT handle, name, role FROM app_user WHERE id = 1")
            .fetch_one(&state.db)
            .await?;
    Ok(Json(WhoamiResp {
        user: UserBrief {
            handle: row.0,
            name: row.1,
            role: row.2,
        },
        token: TokenBrief {
            name: pat.name.clone(),
            scopes: pat.scopes.clone(),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::handler;
    use crate::test_support::{app_state, noop_notify};
    use axum::{extract::State, Extension};
    use ep_auth::AuthPat;

    #[tokio::test]
    async fn handler_returns_owner_and_authenticated_token_metadata() {
        let db = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::query(
            "CREATE TABLE app_user (
                id INTEGER PRIMARY KEY,
                handle TEXT NOT NULL,
                name TEXT NOT NULL,
                role TEXT NOT NULL
            )",
        )
        .execute(&db)
        .await
        .expect("schema");
        sqlx::query(
            "INSERT INTO app_user (id, handle, name, role) VALUES (1, 'admin', 'Owner', 'OWNER')",
        )
        .execute(&db)
        .await
        .expect("user");
        let state = app_state(db, noop_notify());
        let pat = AuthPat {
            id: 42,
            name: "iOS Shortcuts".into(),
            scopes: vec![
                ep_core::SCOPE_FIN_WRITE.into(),
                ep_core::SCOPE_NOTIFY_WRITE.into(),
            ],
        };

        let axum::Json(resp) = handler(State(state), Extension(pat))
            .await
            .expect("whoami response");

        assert_eq!(resp.user.handle, "admin");
        assert_eq!(resp.user.name, "Owner");
        assert_eq!(resp.user.role, "OWNER");
        assert_eq!(resp.token.name, "iOS Shortcuts");
        assert_eq!(
            resp.token.scopes,
            [ep_core::SCOPE_FIN_WRITE, ep_core::SCOPE_NOTIFY_WRITE]
        );
    }
}
