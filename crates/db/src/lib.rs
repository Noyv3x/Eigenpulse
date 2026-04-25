#[cfg(feature = "ssr")]
pub mod pool;

#[cfg(feature = "ssr")]
pub use pool::open_pool;

#[cfg(feature = "ssr")]
pub static CORE_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");
