use crate::{AppState, Module, NavEntry, ModuleLink};
use sqlx::SqlitePool;

pub struct ModuleRegistry {
    items: Vec<&'static dyn Module>,
}

impl Default for ModuleRegistry {
    fn default() -> Self { Self::new() }
}

impl ModuleRegistry {
    pub const fn new() -> Self { Self { items: Vec::new() } }

    pub fn with(mut self, m: &'static dyn Module) -> Self {
        self.items.push(m);
        self
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static dyn Module> + '_ {
        self.items.iter().copied()
    }

    pub fn find(&self, code: &str) -> Option<&'static dyn Module> {
        self.iter().find(|m| m.code().eq_ignore_ascii_case(code))
    }

    pub async fn run_migrations(&self, pool: &SqlitePool) -> anyhow::Result<()> {
        for m in self.iter() {
            for (name, sql) in m.migrations() {
                let already: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM _ep_module_migration WHERE module = ?1 AND name = ?2"
                )
                .bind(m.code())
                .bind(*name)
                .fetch_optional(pool)
                .await?;
                if already.is_some() { continue; }
                let mut tx = pool.begin().await?;
                // Allow multi-statement sql via execute_many
                for stmt in split_sql(sql) {
                    if stmt.trim().is_empty() { continue; }
                    sqlx::query(&stmt).execute(&mut *tx).await?;
                }
                sqlx::query(
                    "INSERT INTO _ep_module_migration(module, name) VALUES (?1, ?2)"
                )
                .bind(m.code())
                .bind(*name)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                tracing::info!(module = m.code(), name = name, "applied module migration");
            }
        }
        Ok(())
    }

    pub fn web_router(&self, state: AppState) -> axum::Router<AppState> {
        let mut r = axum::Router::new();
        for m in self.iter() {
            r = r.merge(m.routes(state.clone()));
        }
        r
    }

    pub fn open_api_router(&self, state: AppState) -> axum::Router<AppState> {
        let mut r = axum::Router::new();
        for m in self.iter() {
            let sub = m.open_api(state.clone());
            r = r.nest(&format!("/{}", m.code().to_ascii_lowercase()), sub);
        }
        r
    }

    pub fn nav(&self) -> Vec<NavEntry> {
        let mut entries: Vec<NavEntry> = self
            .iter()
            .map(|m| NavEntry {
                code: m.code(),
                name: m.name(),
                name_cn: m.name_cn(),
                icon: m.nav_icon(),
                section: m.nav_section(),
                path: route_path(m.code()),
            })
            .collect();
        entries.sort_by_key(|e| e.section.order());
        entries
    }

    pub fn all_links(&self) -> Vec<&'static ModuleLink> {
        self.iter().flat_map(|m| m.links().iter()).collect()
    }

    pub fn all_scopes(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = self.iter().flat_map(|m| m.open_api_scopes().iter().copied()).collect();
        out.sort();
        out.dedup();
        out
    }
}

fn route_path(code: &str) -> String {
    match code {
        "DSH" => "/".into(),
        c => format!("/{}", c.to_ascii_lowercase()),
    }
}

fn split_sql(sql: &str) -> Vec<String> {
    // Naive splitter: split on `;` not inside single-quoted string. Sufficient for migration SQL we author.
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_quote = false;
    for ch in sql.chars() {
        if ch == '\'' { in_quote = !in_quote; }
        if ch == ';' && !in_quote {
            out.push(buf.trim().to_string());
            buf.clear();
        } else {
            buf.push(ch);
        }
    }
    if !buf.trim().is_empty() { out.push(buf.trim().to_string()); }
    out
}
