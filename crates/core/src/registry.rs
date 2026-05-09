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
        let mut r = axum::Router::new();
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
        let already: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM _ep_module_migration WHERE module = ?1 AND name = ?2",
        )
        .bind(module.code())
        .bind(*name)
        .fetch_optional(pool)
        .await?;
        if already.is_some() {
            continue;
        }
        let mut tx = pool.begin().await?;
        // Allow module migrations to embed multiple top-level statements.
        for stmt in split_sql(sql) {
            if stmt.trim().is_empty() {
                continue;
            }
            sqlx::query(&stmt).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO _ep_module_migration(module, name) VALUES (?1, ?2)")
            .bind(module.code())
            .bind(*name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        tracing::info!(
            module = module.code(),
            name = name,
            "applied module migration"
        );
    }
    Ok(())
}

/// Split a SQLite migration script into top-level statements.
///
/// Semicolons inside string literals, quoted identifiers, and comments are
/// preserved. This is intentionally small and SQLite-focused; it exists so the
/// production registry and focused module tests apply migrations the same way.
pub fn split_sql(sql: &str) -> Vec<String> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Normal,
        SingleQuoted,
        DoubleQuoted,
        BacktickQuoted,
        BracketQuoted,
        LineComment,
        BlockComment,
    }

    let mut out = Vec::new();
    let mut buf = String::new();
    let mut chars = sql.chars().peekable();
    let mut state = State::Normal;

    while let Some(ch) = chars.next() {
        match state {
            State::Normal => match ch {
                ';' => {
                    let stmt = buf.trim();
                    if !stmt.is_empty() {
                        out.push(stmt.to_string());
                    }
                    buf.clear();
                }
                '\'' => {
                    state = State::SingleQuoted;
                    buf.push(ch);
                }
                '"' => {
                    state = State::DoubleQuoted;
                    buf.push(ch);
                }
                '`' => {
                    state = State::BacktickQuoted;
                    buf.push(ch);
                }
                '[' => {
                    state = State::BracketQuoted;
                    buf.push(ch);
                }
                '-' if chars.peek() == Some(&'-') => {
                    state = State::LineComment;
                    buf.push(ch);
                    if let Some(next) = chars.next() {
                        buf.push(next);
                    }
                }
                '/' if chars.peek() == Some(&'*') => {
                    state = State::BlockComment;
                    buf.push(ch);
                    if let Some(next) = chars.next() {
                        buf.push(next);
                    }
                }
                _ => buf.push(ch),
            },
            State::SingleQuoted => {
                buf.push(ch);
                if ch == '\'' {
                    if chars.peek() == Some(&'\'') {
                        if let Some(next) = chars.next() {
                            buf.push(next);
                        }
                    } else {
                        state = State::Normal;
                    }
                }
            }
            State::DoubleQuoted => {
                buf.push(ch);
                if ch == '"' {
                    if chars.peek() == Some(&'"') {
                        if let Some(next) = chars.next() {
                            buf.push(next);
                        }
                    } else {
                        state = State::Normal;
                    }
                }
            }
            State::BacktickQuoted => {
                buf.push(ch);
                if ch == '`' {
                    state = State::Normal;
                }
            }
            State::BracketQuoted => {
                buf.push(ch);
                if ch == ']' {
                    state = State::Normal;
                }
            }
            State::LineComment => {
                buf.push(ch);
                if ch == '\n' {
                    state = State::Normal;
                }
            }
            State::BlockComment => {
                buf.push(ch);
                if ch == '*' && chars.peek() == Some(&'/') {
                    if let Some(next) = chars.next() {
                        buf.push(next);
                    }
                    state = State::Normal;
                }
            }
        }
    }

    let stmt = buf.trim();
    if !stmt.is_empty() {
        out.push(stmt.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sql_splits_top_level_semicolons() {
        assert_eq!(
            split_sql("CREATE TABLE a(id INTEGER); INSERT INTO a VALUES (1);"),
            vec!["CREATE TABLE a(id INTEGER)", "INSERT INTO a VALUES (1)"]
        );
    }

    #[test]
    fn split_sql_ignores_semicolons_inside_string_literals() {
        assert_eq!(
            split_sql("INSERT INTO a VALUES ('one;two', 'it''s ok; still string'); SELECT 1;"),
            vec![
                "INSERT INTO a VALUES ('one;two', 'it''s ok; still string')",
                "SELECT 1"
            ]
        );
    }

    #[test]
    fn split_sql_ignores_semicolons_inside_comments() {
        assert_eq!(
            split_sql(
                "-- seed; comment\nINSERT INTO a VALUES (1); /* note; still comment */ SELECT 2;"
            ),
            vec![
                "-- seed; comment\nINSERT INTO a VALUES (1)",
                "/* note; still comment */ SELECT 2"
            ]
        );
    }

    #[test]
    fn split_sql_ignores_semicolons_inside_quoted_identifiers() {
        assert_eq!(
            split_sql(r#"CREATE TABLE "semi;colon"([x;y] TEXT, `z;w` TEXT);"#),
            vec![r#"CREATE TABLE "semi;colon"([x;y] TEXT, `z;w` TEXT)"#]
        );
    }

    #[test]
    fn split_sql_skips_empty_statements() {
        assert_eq!(split_sql(";; SELECT 1; ;"), vec!["SELECT 1"]);
    }
}
