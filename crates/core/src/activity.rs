use sqlx::SqlitePool;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TodayActivityOrder {
    Asc,
    Desc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TodayActivity {
    pub date: String,
    pub rows: Vec<TodayActivityRow>,
}

/// Mirrors the `activity` table column-for-column so `sqlx::FromRow` can
/// decode a row straight into it — no hand-written tuple mapping.
///
/// Finance activity amounts are signed integer minor units stored as text,
/// matching `fin_txn.amount`; non-finance activity rows keep `amount = NULL`.
#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct TodayActivityRow {
    pub occurred_at: i64,
    pub module: String,
    pub doc_id: String,
    pub summary: String,
    pub amount: Option<crate::MinorAmount>,
    /// Currency of the source transaction (finance rows only); `None` for
    /// non-finance modules and for finance rows with no monetary amount.
    pub currency_code: Option<String>,
    pub status: Option<String>,
    pub link_doc: Option<String>,
}

pub async fn load_today_activity(
    pool: &SqlitePool,
    order: TodayActivityOrder,
    limit: Option<u32>,
) -> sqlx::Result<TodayActivity> {
    load_today_activity_paged(pool, order, limit, 0).await
}

/// Like [`load_today_activity`] but with an additional `offset` for keyset-free
/// limit/offset pagination (used by the `/api/v1/today` open-API endpoint).
/// A `None` limit returns every row from `offset` onward; `offset` is clamped
/// to be non-negative. `limit` is clamped to at least 1 when present, matching
/// the existing zero-limit behaviour.
pub async fn load_today_activity_paged(
    pool: &SqlitePool,
    order: TodayActivityOrder,
    limit: Option<u32>,
    offset: u32,
) -> sqlx::Result<TodayActivity> {
    let date: String = sqlx::query_scalar("SELECT date('now','localtime')")
        .fetch_one(pool)
        .await?;

    // "Today" follows local wall-clock time. SQLite modifiers compose
    // left-to-right: shift to localtime, round to local midnight, then convert
    // back to UTC epoch seconds for comparison against stored unix timestamps.
    // `direction` is an enum-derived literal (never user input) and the
    // `LIMIT`/`OFFSET` values are bound, so this format! is injection-safe.
    //
    // SQLite requires a LIMIT before OFFSET; when the caller wants an offset
    // but no row cap we use the sentinel `LIMIT -1` (== unbounded) so paging
    // past a cursor without a cap still works.
    let direction = match order {
        TodayActivityOrder::Asc => "ASC",
        TodayActivityOrder::Desc => "DESC",
    };
    let mut sql = format!(
        "SELECT occurred_at, module, doc_id, summary, amount, currency_code, status, link_doc
           FROM activity
          WHERE occurred_at >= unixepoch('now','localtime','start of day','utc')
          ORDER BY occurred_at {direction}"
    );
    let want_offset = offset > 0;
    if limit.is_some() || want_offset {
        sql.push_str(" LIMIT ?1");
    }
    if want_offset {
        sql.push_str(" OFFSET ?2");
    }

    let mut query = sqlx::query_as::<_, TodayActivityRow>(&sql);
    if limit.is_some() || want_offset {
        // `-1` is SQLite's documented "no limit" sentinel for the OFFSET case.
        let bound_limit = limit.map(|l| i64::from(l.max(1))).unwrap_or(-1);
        query = query.bind(bound_limit);
    }
    if want_offset {
        query = query.bind(i64::from(offset));
    }
    let rows = query.fetch_all(pool).await?;

    Ok(TodayActivity { date, rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool_with_activity() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("pool");
        sqlx::query(
            "CREATE TABLE activity (
                occurred_at INTEGER NOT NULL,
                module TEXT NOT NULL,
                doc_id TEXT NOT NULL,
                summary TEXT NOT NULL,
                amount TEXT,
                currency_code TEXT,
                status TEXT,
                link_doc TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("schema");
        pool
    }

    #[tokio::test]
    async fn load_today_activity_orders_and_limits_rows() {
        let pool = pool_with_activity().await;
        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, status, link_doc)
             VALUES
                (unixepoch('now') - 20, 'FIN', 'FIN-1', 'summary', '-100', NULL, NULL),
                (unixepoch('now') - 10, 'FIN', 'FIT-1', 'summary', '-100', NULL, NULL)",
        )
        .execute(&pool)
        .await
        .expect("activity");

        let asc = load_today_activity(&pool, TodayActivityOrder::Asc, None)
            .await
            .expect("asc");
        assert_eq!(asc.rows[0].doc_id, "FIN-1");
        assert_eq!(asc.rows[1].doc_id, "FIT-1");

        let desc = load_today_activity(&pool, TodayActivityOrder::Desc, Some(1))
            .await
            .expect("desc");
        assert_eq!(desc.rows.len(), 1);
        assert_eq!(desc.rows[0].doc_id, "FIT-1");
    }

    #[tokio::test]
    async fn load_today_activity_paged_applies_limit_and_offset() {
        let pool = pool_with_activity().await;
        // Three rows, oldest → newest by occurred_at.
        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, status, link_doc)
             VALUES
                (unixepoch('now') - 30, 'FIN', 'A', 's', '-1', NULL, NULL),
                (unixepoch('now') - 20, 'FIN', 'B', 's', '-1', NULL, NULL),
                (unixepoch('now') - 10, 'FIN', 'C', 's', '-1', NULL, NULL)",
        )
        .execute(&pool)
        .await
        .expect("activity");

        // DESC = C, B, A. Page 2 with limit=1, offset=1 → B.
        let page = load_today_activity_paged(&pool, TodayActivityOrder::Desc, Some(1), 1)
            .await
            .expect("page");
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].doc_id, "B");

        // Offset past the end yields no rows.
        let empty = load_today_activity_paged(&pool, TodayActivityOrder::Desc, Some(10), 5)
            .await
            .expect("empty page");
        assert!(empty.rows.is_empty());

        // Offset with no limit (sentinel LIMIT -1) skips and returns the rest.
        let rest = load_today_activity_paged(&pool, TodayActivityOrder::Desc, None, 1)
            .await
            .expect("rest");
        assert_eq!(
            rest.rows
                .iter()
                .map(|r| r.doc_id.as_str())
                .collect::<Vec<_>>(),
            vec!["B", "A"]
        );
    }

    #[tokio::test]
    async fn load_today_activity_clamps_zero_limit_to_one() {
        let pool = pool_with_activity().await;
        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, status, link_doc)
             VALUES
                (unixepoch('now') - 20, 'FIN', 'FIN-1', 'summary', '-1', NULL, NULL),
                (unixepoch('now') - 10, 'FIN', 'FIT-1', 'summary', '-1', NULL, NULL)",
        )
        .execute(&pool)
        .await
        .expect("activity");

        let desc = load_today_activity(&pool, TodayActivityOrder::Desc, Some(0))
            .await
            .expect("desc");

        assert_eq!(desc.rows.len(), 1);
        assert_eq!(desc.rows[0].doc_id, "FIT-1");
    }
}
