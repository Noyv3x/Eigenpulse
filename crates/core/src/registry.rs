use crate::{AppState, Module};
use sqlx::SqlitePool;

pub struct ModuleRegistry {
    items: Vec<&'static dyn Module>,
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleRegistry {
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn with(mut self, m: &'static dyn Module) -> Self {
        self.items.push(m);
        self
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static dyn Module> + '_ {
        self.items.iter().copied()
    }

    pub async fn run_migrations(&self, pool: &SqlitePool) -> anyhow::Result<()> {
        for m in self.iter() {
            run_module_migrations(pool, m).await?;
        }
        Ok(())
    }

    pub fn open_api_router(&self, state: AppState) -> axum::Router<AppState> {
        let mut r = axum::Router::<AppState>::new();
        for m in self.iter() {
            let sub = m.open_api(state.clone());
            r = r.nest(&format!("/{}", m.code().to_ascii_lowercase()), sub);
        }
        r
    }
}

/// Apply one module's migrations through the same filename-keyed ledger used
/// by production startup.
pub async fn run_module_migrations(pool: &SqlitePool, module: &dyn Module) -> anyhow::Result<()> {
    for (name, sql) in module.migrations() {
        let mut tx = pool.begin().await?;
        let claimed: Option<i64> = sqlx::query_scalar(
            r#"
            INSERT INTO _ep_module_migration(module, name) VALUES (?1, ?2)
            ON CONFLICT(module, name) DO NOTHING
            RETURNING 1
            "#,
        )
        .bind(module.code())
        .bind(*name)
        .fetch_optional(&mut *tx)
        .await?;

        if claimed.is_none() {
            tx.commit().await?;
            continue;
        }

        sqlx::raw_sql(sql).execute(&mut *tx).await?;
        tx.commit().await?;
        tracing::info!(
            module = module.code(),
            name = name,
            "applied module migration"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestModule {
        code: &'static str,
        migrations: &'static [(&'static str, &'static str)],
    }

    impl Module for TestModule {
        fn code(&self) -> &'static str {
            self.code
        }

        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            self.migrations
        }
    }

    async fn pool_with_ledger() -> anyhow::Result<SqlitePool> {
        let pool = SqlitePool::connect("sqlite::memory:").await?;
        sqlx::query(
            "CREATE TABLE _ep_module_migration (
                module TEXT NOT NULL,
                name TEXT NOT NULL,
                applied_at INTEGER NOT NULL DEFAULT (unixepoch()),
                PRIMARY KEY (module, name)
            )",
        )
        .execute(&pool)
        .await?;
        Ok(pool)
    }

    #[tokio::test]
    async fn run_module_migrations_executes_raw_sql_and_is_idempotent() -> anyhow::Result<()> {
        const MIGRATIONS: &[(&str, &str)] = &[(
            "001_trigger",
            r#"
            CREATE TABLE raw_parent(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE raw_log(value TEXT NOT NULL);
            CREATE TRIGGER raw_parent_ai AFTER INSERT ON raw_parent
            BEGIN
                INSERT INTO raw_log(value) VALUES (new.value || ';logged');
            END;
            INSERT INTO raw_parent(value) VALUES ('one;two');
            "#,
        )];
        let module = TestModule {
            code: "TST",
            migrations: MIGRATIONS,
        };
        let pool = pool_with_ledger().await?;

        run_module_migrations(&pool, &module).await?;
        run_module_migrations(&pool, &module).await?;

        let log: String = sqlx::query_scalar("SELECT value FROM raw_log")
            .fetch_one(&pool)
            .await?;
        assert_eq!(log, "one;two;logged");
        let parent_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM raw_parent")
            .fetch_one(&pool)
            .await?;
        assert_eq!(parent_count, 1);
        let ledger_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _ep_module_migration WHERE module = 'TST'")
                .fetch_one(&pool)
                .await?;
        assert_eq!(ledger_count, 1);
        Ok(())
    }

    #[tokio::test]
    async fn failed_module_migration_rolls_back_ledger_claim() -> anyhow::Result<()> {
        const MIGRATIONS: &[(&str, &str)] = &[(
            "001_broken",
            "CREATE TABLE before_fail(id INTEGER); THIS IS NOT SQL;",
        )];
        let module = TestModule {
            code: "BAD",
            migrations: MIGRATIONS,
        };
        let pool = pool_with_ledger().await?;

        let result = run_module_migrations(&pool, &module).await;
        assert!(result.is_err());

        let ledger_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _ep_module_migration WHERE module = 'BAD'")
                .fetch_one(&pool)
                .await?;
        assert_eq!(ledger_count, 0);
        let table_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'before_fail'",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(table_count, 0);
        Ok(())
    }
}
