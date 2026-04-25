use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("not found")]
    NotFound,
    #[error("validation: {0}")]
    Validation(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("config: {0}")]
    Config(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl Error {
    pub fn http_status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::Unauthorized => 401,
            Self::Forbidden => 403,
            Self::Validation(_) => 400,
            Self::Conflict(_) => 409,
            _ => 500,
        }
    }
}
