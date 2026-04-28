use crate::model::*;
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerData {
    pub accounts: Vec<Account>,
    pub categories: Vec<Category>,
    pub txns: Vec<Txn>,
    pub category_summary: Vec<CategorySummary>,
    pub month: MonthSummary,
}

#[server(LoadLedger, "/api/_internal/fin", "Url", "load_ledger")]
pub async fn load_ledger() -> Result<LedgerData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state: ep_core::AppState = expect_context();
        let pool = &state.db;

        type AccRow  = (String, String, String, String, f64);
        type CatRow  = (String, String, String, i64);
        type TxnRow  = (String, i64, String, String, String, f64, String, Option<String>, Option<String>);
        type SumRow  = (String, f64);

        let accounts_q = sqlx::query_as::<_, AccRow>(
            "SELECT code, name, type, tone, balance FROM fin_account WHERE archived = 0 ORDER BY code"
        ).fetch_all(pool);
        let categories_q = sqlx::query_as::<_, CatRow>(
            "SELECT code, name, tone, sort_order FROM fin_category ORDER BY sort_order"
        ).fetch_all(pool);
        let txns_q = sqlx::query_as::<_, TxnRow>(
            "SELECT doc_id, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id
               FROM fin_txn ORDER BY occurred_at DESC LIMIT 50"
        ).fetch_all(pool);
        let cat_sum_q = sqlx::query_as::<_, SumRow>(
            "SELECT category_code, SUM(-amount)
               FROM fin_txn
              WHERE amount < 0 AND occurred_at >= unixepoch('now','start of month')
              GROUP BY category_code"
        ).fetch_all(pool);
        let income_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(amount),0) FROM fin_txn
              WHERE amount > 0 AND tag = 'inc' AND occurred_at >= unixepoch('now','start of month')"
        ).fetch_one(pool);
        let expense_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(-amount),0) FROM fin_txn
              WHERE amount < 0 AND occurred_at >= unixepoch('now','start of month')"
        ).fetch_one(pool);
        let budget_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(amount),0) FROM fin_budget WHERE period = strftime('%Y-%m','now')"
        ).fetch_one(pool);

        let (accounts, categories, txns_rows, cat_rows, income, expense, budget_total) =
            tokio::try_join!(accounts_q, categories_q, txns_q, cat_sum_q, income_q, expense_q, budget_q)
                .map_err(internal)?;

        let accounts = accounts.into_iter()
            .map(|r| Account { code: r.0, name: r.1, r#type: r.2, tone: r.3, balance: r.4 })
            .collect::<Vec<_>>();
        let categories = categories.into_iter()
            .map(|r| Category { code: r.0, name: r.1, tone: r.2, sort_order: r.3 })
            .collect::<Vec<_>>();
        let txns = txns_rows.into_iter().map(|r| Txn {
            doc_id: r.0, occurred_at: r.1, merchant: r.2, category_code: r.3, account_code: r.4,
            amount: r.5, tag: r.6, note: r.7, linked_doc_id: r.8,
        }).collect::<Vec<_>>();

        let total_spent: f64 = cat_rows.iter().map(|(_, v)| *v).sum();
        let category_summary = cat_rows.into_iter().map(|(code, value)| {
            let cat = categories.iter().find(|c| c.code == code);
            CategorySummary {
                code: code.clone(),
                name: cat.map(|c| c.name.clone()).unwrap_or_default(),
                tone: cat.map(|c| c.tone.clone()).unwrap_or_default(),
                value,
                pct: if total_spent > 0.0 { (value / total_spent * 1000.0).round() / 10.0 } else { 0.0 },
            }
        }).collect::<Vec<_>>();

        let balance: f64 = accounts.iter().map(|a| a.balance).sum();

        Ok(LedgerData {
            accounts, categories, txns, category_summary,
            month: MonthSummary {
                income, expense, savings: income - expense, balance,
                balance_delta: 1284.50, budget_used: expense, budget_total,
            },
        })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

#[server(AddTxn, "/api/_internal/fin", "Url", "add_txn")]
pub async fn add_txn(
    merchant: String,
    category_code: String,
    account_code: String,
    amount: f64,
    tag: String,
    note: String,
    linked_doc_id: String,
) -> Result<Txn, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;

        let merchant = merchant.trim().to_string();
        if merchant.is_empty() {
            return Err(ServerFnError::Args("merchant is required".into()));
        }
        let tag_kind = match crate::model::Tag::parse(&tag) {
            Some(k) => k,
            None => return Err(ServerFnError::Args(format!("tag must be exp/inc/tfr, got '{tag}'"))),
        };
        if category_code.trim().is_empty() {
            return Err(ServerFnError::Args("category_code is required".into()));
        }
        if account_code.trim().is_empty() {
            return Err(ServerFnError::Args("account_code is required".into()));
        }
        // Form contract: positive amount, `tag` carries the sign (matches the
        // seed convention). `/api/v1/fin/txn` is a *separate* code path that
        // accepts pre-signed amounts — don't conflate the two.
        if !amount.is_finite() || amount < 0.005 {
            return Err(ServerFnError::Args("amount must be a positive number".into()));
        }
        let amount = if tag_kind == crate::model::Tag::Exp { -amount } else { amount };
        let note_opt = if note.trim().is_empty() { None } else { Some(note.clone()) };
        let linked_opt = if linked_doc_id.trim().is_empty() { None } else { Some(linked_doc_id.clone()) };

        let state: ep_core::AppState = expect_context();
        let pool = &state.db;
        let occurred = time::OffsetDateTime::now_utc().unix_timestamp();

        let mut tx = pool.begin().await.map_err(internal)?;
        let doc_id = ep_core::next_doc_id(&mut tx, "FIN", ep_core::DocIdShape::YearSerial5)
            .await.map_err(internal)?;
        sqlx::query(
            "INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        )
        .bind(&doc_id).bind(occurred).bind(&merchant).bind(&category_code)
        .bind(&account_code).bind(amount).bind(&tag)
        .bind(&note_opt).bind(&linked_opt)
        .execute(&mut *tx).await.map_err(internal)?;

        sqlx::query(
            "UPDATE fin_account SET balance = balance + ?1 WHERE code = ?2"
        ).bind(amount).bind(&account_code).execute(&mut *tx).await.map_err(internal)?;

        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, link_doc)
             VALUES (?1, 'FIN', ?2, ?3, ?4, ?5)"
        ).bind(occurred).bind(&doc_id).bind(&merchant).bind(amount).bind(&linked_opt)
         .execute(&mut *tx).await.map_err(internal)?;

        if let Some(link) = &linked_opt {
            sqlx::query("INSERT OR IGNORE INTO module_link (source_doc, target_doc, kind) VALUES (?1, ?2, 'ref')")
                .bind(&doc_id).bind(link).execute(&mut *tx).await.map_err(internal)?;
        }

        tx.commit().await.map_err(internal)?;

        if amount < -500.0 {
            let n = ep_core::NotifyMessage::warn(format!("大额支出 · {merchant}"))
                .module("FIN")
                .body(format!("¥{:.2} ({})", amount.abs(), category_code))
                .doc_ref(doc_id.clone())
                .link("/finance");
            let _ = state.notify.dispatch(n).await;
        }

        Ok(Txn {
            doc_id, occurred_at: occurred, merchant,
            category_code, account_code,
            amount, tag, note: note_opt, linked_doc_id: linked_opt,
        })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

#[server(DeleteTxn, "/api/_internal/fin", "Url", "delete_txn")]
pub async fn delete_txn(doc_id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state: ep_core::AppState = expect_context();
        let pool = &state.db;
        let mut tx = pool.begin().await.map_err(internal)?;
        // Reverse the balance change first.
        let row: Option<(f64, String)> = sqlx::query_as(
            "SELECT amount, account_code FROM fin_txn WHERE doc_id = ?1"
        ).bind(&doc_id).fetch_optional(&mut *tx).await.map_err(internal)?;
        if let Some((amount, account_code)) = row {
            sqlx::query("UPDATE fin_account SET balance = balance - ?1 WHERE code = ?2")
                .bind(amount).bind(&account_code).execute(&mut *tx).await.map_err(internal)?;
        }
        sqlx::query("DELETE FROM fin_txn WHERE doc_id = ?1").bind(&doc_id).execute(&mut *tx).await.map_err(internal)?;
        sqlx::query("DELETE FROM activity WHERE module = 'FIN' AND doc_id = ?1").bind(&doc_id).execute(&mut *tx).await.map_err(internal)?;
        sqlx::query("DELETE FROM module_link WHERE source_doc = ?1").bind(&doc_id).execute(&mut *tx).await.map_err(internal)?;
        tx.commit().await.map_err(internal)?;
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

#[cfg(feature = "ssr")]
fn internal<E: std::fmt::Display>(e: E) -> ServerFnError {
    ServerFnError::ServerError(e.to_string())
}
