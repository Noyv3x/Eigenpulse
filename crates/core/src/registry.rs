use crate::{AppState, Module};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

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
        self.validate_definition()?;
        self.validate_applied(pool).await?;
        for m in self.iter() {
            run_module_migrations(pool, m).await?;
        }
        Ok(())
    }

    /// Return whether any registered migration is absent from the module
    /// ledger. This is intentionally read-only so startup can take a database
    /// snapshot before [`Self::run_migrations`] performs its first write.
    pub async fn has_pending_migrations(&self, pool: &SqlitePool) -> anyhow::Result<bool> {
        self.validate_definition()?;
        let ledger_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = '_ep_module_migration')",
        )
        .fetch_one(pool)
        .await?;
        if !ledger_exists {
            return Ok(self.iter().any(|module| !module.migrations().is_empty()));
        }

        self.validate_applied(pool).await?;
        let applied: HashMap<(String, String), String> =
            sqlx::query_as("SELECT module, name, checksum FROM _ep_module_migration")
                .fetch_all(pool)
                .await?
                .into_iter()
                .map(|(module, name, checksum)| ((module, name), checksum))
                .collect();
        Ok(self.iter().any(|module| {
            module.migrations().iter().any(|(name, sql)| {
                applied
                    .get(&(module.code().to_string(), (*name).to_string()))
                    .is_none_or(|checksum| checksum != &sql_checksum(sql))
            })
        }))
    }

    pub fn open_api_router(&self, state: AppState) -> axum::Router<AppState> {
        let mut r = axum::Router::<AppState>::new();
        for m in self.iter() {
            let sub = m.open_api(state.clone());
            r = r.nest(&format!("/{}", m.code().to_ascii_lowercase()), sub);
        }
        r
    }

    fn validate_definition(&self) -> anyhow::Result<()> {
        let mut codes = HashSet::new();
        for module in self.iter() {
            let code = module.code();
            if code.is_empty()
                || !code
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            {
                anyhow::bail!(
                    "module slug `{code}` must contain only ASCII letters, digits, underscore, or hyphen"
                );
            }
            if !codes.insert(code.to_ascii_uppercase()) {
                anyhow::bail!("duplicate module code `{code}`");
            }

            let mut previous = None;
            let mut names = HashSet::new();
            for (name, _) in module.migrations() {
                let order = migration_order(name).ok_or_else(|| {
                    anyhow::anyhow!(
                        "module {code} migration `{name}` must match NNN_lowercase_name"
                    )
                })?;
                if !names.insert(*name) {
                    anyhow::bail!("module {code} contains duplicate migration `{name}`");
                }
                if previous.is_some_and(|last| order <= last) {
                    anyhow::bail!(
                        "module {code} migrations must be registered in strictly increasing order"
                    );
                }
                previous = Some(order);
            }
        }
        Ok(())
    }

    async fn validate_applied(&self, pool: &SqlitePool) -> anyhow::Result<()> {
        let applied: Vec<(String, String, String)> =
            sqlx::query_as("SELECT module, name, checksum FROM _ep_module_migration")
                .fetch_all(pool)
                .await?;
        let registered: HashMap<&str, Vec<(&str, &str)>> = self
            .iter()
            .map(|module| (module.code(), module.migrations().to_vec()))
            .collect();
        let mut applied_by_module = HashMap::<String, Vec<(String, String)>>::new();
        for (module, name, checksum) in applied {
            let Some(known) = registered.get(module.as_str()) else {
                anyhow::bail!(
                    "database contains migration for unknown module {module}; refusing to start"
                );
            };
            let Some((_, sql)) = known.iter().find(|(known, _)| *known == name) else {
                anyhow::bail!(
                    "database contains unknown migration {module}/{name}; refusing to run an older binary"
                );
            };
            let expected = sql_checksum(sql);
            if checksum.is_empty() {
                anyhow::bail!("module migration {module}/{name} has an empty checksum");
            }
            if checksum != expected {
                anyhow::bail!(
                    "module migration checksum mismatch for {module}/{name}: database={checksum}, binary={expected}"
                );
            }
            applied_by_module
                .entry(module)
                .or_default()
                .push((name, checksum));
        }

        for (module, expected) in &registered {
            let mut actual = applied_by_module.remove(*module).unwrap_or_default();
            actual.sort_by(|left, right| left.0.cmp(&right.0));
            if actual.len() > expected.len()
                || actual
                    .iter()
                    .zip(expected.iter())
                    .any(|((actual, _), (wanted, _))| actual != wanted)
            {
                let names = actual
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                anyhow::bail!(
                    "applied migrations for module {module} are not a strict registered prefix: [{names}]"
                );
            }
        }
        Ok(())
    }
}

/// Apply one module's migrations through the checksum-enforced ledger used
/// by production startup.
pub async fn run_module_migrations(pool: &SqlitePool, module: &dyn Module) -> anyhow::Result<()> {
    for (name, sql) in module.migrations() {
        let checksum = sql_checksum(sql);
        // Serialise startup writers so two instances cannot both observe a
        // missing ledger row and race while applying the same schema change.
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let apply_result: anyhow::Result<bool> = async {
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT checksum FROM _ep_module_migration WHERE module = ?1 AND name = ?2",
            )
            .bind(module.code())
            .bind(*name)
            .fetch_optional(&mut *tx)
            .await?;

            if let Some(existing) = existing {
                if existing.is_empty() {
                    anyhow::bail!(
                        "module migration {}/{} has an empty checksum",
                        module.code(),
                        name
                    );
                } else if existing != checksum {
                    anyhow::bail!(
                        "module migration checksum mismatch for {}/{}",
                        module.code(),
                        name
                    );
                }
                return Ok(false);
            }

            sqlx::raw_sql(sql).execute(&mut *tx).await?;
            sqlx::query(
                "INSERT INTO _ep_module_migration(module, name, checksum) VALUES (?1, ?2, ?3)",
            )
            .bind(module.code())
            .bind(*name)
            .bind(&checksum)
            .execute(&mut *tx)
            .await?;
            Ok(true)
        }
        .await;

        let applied = match apply_result {
            Ok(applied) => applied,
            Err(error) => {
                if let Err(rollback_error) = tx.rollback().await {
                    anyhow::bail!(
                        "module migration {}/{} failed ({error:#}); rollback also failed: {rollback_error}",
                        module.code(),
                        name
                    );
                }
                return Err(error);
            }
        };
        tx.commit().await?;
        if !applied {
            continue;
        }
        tracing::info!(
            module = module.code(),
            name = name,
            "applied module migration"
        );
    }
    Ok(())
}

fn sql_checksum(sql: &str) -> String {
    hex::encode(Sha256::digest(sql.as_bytes()))
}

fn migration_order(name: &str) -> Option<u16> {
    let (prefix, suffix) = name.split_once('_')?;
    if prefix.len() != 3
        || !prefix.bytes().all(|byte| byte.is_ascii_digit())
        || suffix.is_empty()
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return None;
    }
    prefix.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DESCRIPTOR: crate::ModuleDescriptor = crate::ModuleDescriptor {
        slug: "test",
        route: "/test",
        name_key: "test.name",
        description_key: "test.description",
        icon: crate::IconKind::Dashboard,
        read_scope: "test:read",
        write_scope: "test:write",
        read_scope_label_key: "test.scope.read",
        write_scope_label_key: "test.scope.write",
    };

    struct TestModule {
        code: &'static str,
        migrations: &'static [(&'static str, &'static str)],
    }

    impl Module for TestModule {
        fn descriptor(&self) -> &'static crate::ModuleDescriptor {
            &TEST_DESCRIPTOR
        }

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
                checksum TEXT NOT NULL DEFAULT '',
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
        let checksum: String =
            sqlx::query_scalar("SELECT checksum FROM _ep_module_migration WHERE module = 'TST'")
                .fetch_one(&pool)
                .await?;
        assert_eq!(checksum, sql_checksum(MIGRATIONS[0].1));
        Ok(())
    }

    #[tokio::test]
    async fn registry_reports_pending_migrations_without_claiming_them() -> anyhow::Result<()> {
        static MODULE: TestModule = TestModule {
            code: "PND",
            migrations: &[("001_pending", "CREATE TABLE pending_marker(id INTEGER);")],
        };
        let pool = pool_with_ledger().await?;
        let registry = ModuleRegistry::new().with(&MODULE);

        assert!(registry.has_pending_migrations(&pool).await?);
        let ledger_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _ep_module_migration")
            .fetch_one(&pool)
            .await?;
        assert_eq!(
            ledger_count, 0,
            "pending check must not claim the migration"
        );

        registry.run_migrations(&pool).await?;
        assert!(!registry.has_pending_migrations(&pool).await?);
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

    #[tokio::test]
    async fn checksum_mismatch_and_unknown_newer_migration_are_rejected() -> anyhow::Result<()> {
        static MODULE: TestModule = TestModule {
            code: "CHK",
            migrations: &[("001_init", "CREATE TABLE checksum_marker(id INTEGER);")],
        };
        let pool = pool_with_ledger().await?;
        let registry = ModuleRegistry::new().with(&MODULE);
        registry.run_migrations(&pool).await?;

        sqlx::query("UPDATE _ep_module_migration SET checksum = 'tampered' WHERE module = 'CHK'")
            .execute(&pool)
            .await?;
        assert!(registry.run_migrations(&pool).await.is_err());

        sqlx::query("DELETE FROM _ep_module_migration WHERE module = 'CHK'")
            .execute(&pool)
            .await?;
        sqlx::query(
            "INSERT INTO _ep_module_migration(module, name, checksum) VALUES ('CHK','002_future','x')",
        )
        .execute(&pool)
        .await?;
        assert!(registry.has_pending_migrations(&pool).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn unknown_modules_empty_checksums_and_non_prefix_ledgers_are_rejected(
    ) -> anyhow::Result<()> {
        static MODULE: TestModule = TestModule {
            code: "PRF",
            migrations: &[
                ("001_first", "CREATE TABLE prefix_first(id INTEGER);"),
                ("002_second", "CREATE TABLE prefix_second(id INTEGER);"),
            ],
        };
        let pool = pool_with_ledger().await?;
        let registry = ModuleRegistry::new().with(&MODULE);

        sqlx::query(
            "INSERT INTO _ep_module_migration(module,name,checksum) VALUES ('REMOVED','001_old','x')",
        )
        .execute(&pool)
        .await?;
        assert!(registry.has_pending_migrations(&pool).await.is_err());
        sqlx::query("DELETE FROM _ep_module_migration")
            .execute(&pool)
            .await?;

        sqlx::query(
            "INSERT INTO _ep_module_migration(module,name,checksum) VALUES ('PRF','001_first','')",
        )
        .execute(&pool)
        .await?;
        assert!(registry.has_pending_migrations(&pool).await.is_err());
        sqlx::query("DELETE FROM _ep_module_migration")
            .execute(&pool)
            .await?;

        sqlx::query(
            "INSERT INTO _ep_module_migration(module,name,checksum) VALUES ('PRF','002_second',?1)",
        )
        .bind(sql_checksum(MODULE.migrations[1].1))
        .execute(&pool)
        .await?;
        assert!(registry.has_pending_migrations(&pool).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn registry_rejects_duplicate_codes_and_out_of_order_migrations() -> anyhow::Result<()> {
        static FIRST: TestModule = TestModule {
            code: "DUP",
            migrations: &[],
        };
        static SECOND: TestModule = TestModule {
            code: "DUP",
            migrations: &[],
        };
        static OUT_OF_ORDER: TestModule = TestModule {
            code: "ORD",
            migrations: &[("002_second", "SELECT 1;"), ("001_first", "SELECT 1;")],
        };
        let pool = pool_with_ledger().await?;
        assert!(ModuleRegistry::new()
            .with(&FIRST)
            .with(&SECOND)
            .has_pending_migrations(&pool)
            .await
            .is_err());
        assert!(ModuleRegistry::new()
            .with(&OUT_OF_ORDER)
            .has_pending_migrations(&pool)
            .await
            .is_err());
        Ok(())
    }
}
