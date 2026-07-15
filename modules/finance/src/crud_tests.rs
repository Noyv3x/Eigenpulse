//! Finance domain integration tests.
//!
//! These tests apply the complete module migration stack and verify that every
//! business table and foreign key remains inside the `fin_*` namespace.

#![cfg(feature = "ssr")]

use crate::amount::MinorAmount;
use crate::server_fns::{
    AccountPatchFields, AddTransferFields, AddTxnFields, CategoryPatchFields, CurrencyPatchFields,
    TxnPatchFields,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

trait IntoAnyhow<T> {
    fn into_anyhow(self) -> anyhow::Result<T>;
}

impl<T> IntoAnyhow<T> for Result<T, leptos::server_fn::ServerFnError> {
    fn into_anyhow(self) -> anyhow::Result<T> {
        self.map_err(|error| anyhow::anyhow!("{error:?}"))
    }
}

fn amount(value: i64) -> MinorAmount {
    MinorAmount::from(value)
}

fn utc() -> ep_core::AppTimezone {
    ep_core::AppTimezone::utc()
}

fn timezone(name: &str) -> ep_core::AppTimezone {
    ep_core::AppTimezone::parse(name).expect("valid test timezone")
}

async fn apply_finance_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::raw_sql(include_str!("../migrations/001_finance.sql"))
        .execute(pool)
        .await?;
    Ok(())
}

async fn pool() -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?
        .foreign_keys(true)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    apply_finance_migrations(&pool).await?;
    Ok(pool)
}

async fn seed_account(
    pool: &SqlitePool,
    currency_id: i64,
    name: &str,
    opening: i64,
) -> anyhow::Result<crate::model::Account> {
    crate::server_fns::create_account_inner(
        pool,
        currency_id,
        name.into(),
        "Checking".into(),
        "".into(),
        amount(opening),
    )
    .await
    .into_anyhow()
}

async fn seed_category(
    pool: &SqlitePool,
    currency_id: i64,
    name: &str,
) -> anyhow::Result<crate::model::Category> {
    crate::server_fns::create_category_inner(
        pool,
        currency_id,
        name.into(),
        "".into(),
        "".into(),
        0,
    )
    .await
    .into_anyhow()
}

async fn balance(pool: &SqlitePool, account_id: i64) -> anyhow::Result<MinorAmount> {
    Ok(
        sqlx::query_scalar("SELECT balance FROM fin_account WHERE id = ?1")
            .bind(account_id)
            .fetch_one(pool)
            .await?,
    )
}

#[tokio::test]
async fn baseline_uses_integer_ids_and_module_owned_tables() -> anyhow::Result<()> {
    let pool = pool().await?;
    let currency = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    assert!(currency.id > 0);
    assert_eq!(currency.code, "CNY");

    let business_tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_schema
          WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != '_ep_module_migration'
          ORDER BY name",
    )
    .fetch_all(&pool)
    .await?;
    assert!(
        business_tables
            .iter()
            .all(|table| table.starts_with("fin_")),
        "finance baseline contains a table outside its namespace: {business_tables:?}"
    );

    let account = seed_account(&pool, currency.id, "Cash", 0).await?;
    let category = seed_category(&pool, currency.id, "Food").await?;
    assert!(account.id > 0);
    assert!(category.id > 0);
    assert_eq!(account.currency_id, currency.id);
    assert_eq!(category.currency_id, currency.id);
    Ok(())
}

#[tokio::test]
async fn every_business_foreign_key_stays_in_finance() -> anyhow::Result<()> {
    let pool = pool().await?;
    for table in [
        "fin_account",
        "fin_category",
        "fin_transfer",
        "fin_txn",
        "fin_budget",
    ] {
        type ForeignKeyRow = (i64, i64, String, String, String, String, String, String);
        let rows: Vec<ForeignKeyRow> =
            sqlx::query_as(&format!("PRAGMA foreign_key_list('{table}')"))
                .fetch_all(&pool)
                .await?;
        assert!(!rows.is_empty(), "{table} should own explicit foreign keys");
        for row in rows {
            assert!(
                row.2.starts_with("fin_"),
                "{table} unexpectedly references {}",
                row.2
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn transaction_create_update_delete_keeps_balance_exact() -> anyhow::Result<()> {
    let pool = pool().await?;
    let currency = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    let cash = seed_account(&pool, currency.id, "Cash", 10_000).await?;
    let bank = seed_account(&pool, currency.id, "Bank", 20_000).await?;
    let food = seed_category(&pool, currency.id, "Food").await?;
    let now: i64 = sqlx::query_scalar("SELECT unixepoch()")
        .fetch_one(&pool)
        .await?;

    let txn = crate::server_fns::add_txn_inner(
        &pool,
        utc(),
        AddTxnFields {
            currency_id: currency.id,
            merchant: "Market".into(),
            category_id: food.id,
            account_id: cash.id,
            amount: amount(-1_250),
            tag: "exp".into(),
            note: Some("groceries".into()),
            occurred_at: now,
        },
    )
    .await
    .into_anyhow()?;
    assert!(txn.id > 0);
    assert_eq!(txn.category_id, Some(food.id));
    assert_eq!(balance(&pool, cash.id).await?, amount(8_750));

    let updated = crate::server_fns::patch_txn_inner(
        &pool,
        utc(),
        txn.id,
        TxnPatchFields {
            merchant: Some("Bigger Market".into()),
            category_id: Some(food.id),
            account_id: Some(bank.id),
            amount: Some("20".into()),
            note: Some(None),
            occurred_at: Some(now),
        },
    )
    .await
    .into_anyhow()?;
    assert_eq!(updated.account_id, bank.id);
    assert_eq!(balance(&pool, cash.id).await?, amount(10_000));
    assert_eq!(balance(&pool, bank.id).await?, amount(18_000));

    assert!(crate::server_fns::delete_txn_inner(&pool, txn.id)
        .await
        .into_anyhow()?);
    assert_eq!(balance(&pool, bank.id).await?, amount(20_000));
    assert!(!crate::server_fns::delete_txn_inner(&pool, txn.id)
        .await
        .into_anyhow()?);
    Ok(())
}

#[tokio::test]
async fn concurrent_transaction_updates_preserve_balance_invariant() -> anyhow::Result<()> {
    let temp = tempfile::NamedTempFile::new()?;
    let url = format!("sqlite://{}", temp.path().display());
    let options = SqliteConnectOptions::from_str(&url)?
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;
    apply_finance_migrations(&pool).await?;

    let currency = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    let account = seed_account(&pool, currency.id, "Concurrent", 10_000).await?;
    let category = seed_category(&pool, currency.id, "Concurrent expense").await?;
    let now: i64 = sqlx::query_scalar("SELECT unixepoch()")
        .fetch_one(&pool)
        .await?;
    let transaction = crate::server_fns::add_txn_inner(
        &pool,
        utc(),
        AddTxnFields {
            currency_id: currency.id,
            merchant: "initial".into(),
            category_id: category.id,
            account_id: account.id,
            amount: amount(-100),
            tag: "exp".into(),
            note: None,
            occurred_at: now,
        },
    )
    .await
    .into_anyhow()?;

    let first = crate::server_fns::patch_txn_inner(
        &pool,
        utc(),
        transaction.id,
        TxnPatchFields {
            merchant: Some("first".into()),
            category_id: Some(category.id),
            account_id: Some(account.id),
            amount: Some("2".into()),
            note: Some(None),
            occurred_at: Some(now),
        },
    );
    let second = crate::server_fns::patch_txn_inner(
        &pool,
        utc(),
        transaction.id,
        TxnPatchFields {
            merchant: Some("second".into()),
            category_id: Some(category.id),
            account_id: Some(account.id),
            amount: Some("3".into()),
            note: Some(None),
            occurred_at: Some(now),
        },
    );
    let (first, second) = tokio::join!(first, second);
    first.into_anyhow()?;
    second.into_anyhow()?;

    let stored: MinorAmount = sqlx::query_scalar("SELECT amount FROM fin_txn WHERE id = ?1")
        .bind(transaction.id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        balance(&pool, account.id).await?,
        amount(10_000)
            .checked_add(stored)
            .expect("test amount fits")
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_partial_patches_merge_omitted_fields() -> anyhow::Result<()> {
    let temp = tempfile::NamedTempFile::new()?;
    let url = format!("sqlite://{}", temp.path().display());
    let options = SqliteConnectOptions::from_str(&url)?
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;
    apply_finance_migrations(&pool).await?;

    let currency = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    let currency_symbol = crate::server_fns::patch_currency_inner(
        &pool,
        currency.id,
        CurrencyPatchFields {
            symbol: Some("¥".into()),
            ..CurrencyPatchFields::default()
        },
    );
    let currency_metadata = crate::server_fns::patch_currency_inner(
        &pool,
        currency.id,
        CurrencyPatchFields {
            remark: Some("Chinese yuan".into()),
            sort_order: Some(9),
            ..CurrencyPatchFields::default()
        },
    );
    let (currency_symbol, currency_metadata) = tokio::join!(currency_symbol, currency_metadata);
    currency_symbol.into_anyhow()?;
    currency_metadata.into_anyhow()?;
    let patched_currency = crate::server_fns::resolve_currency(&pool, currency.id)
        .await
        .into_anyhow()?;
    assert_eq!(patched_currency.symbol, "¥");
    assert_eq!(patched_currency.remark, "Chinese yuan");
    assert_eq!(patched_currency.sort_order, 9);

    let patch_account = seed_account(&pool, currency.id, "Patch account", 0).await?;
    let account_name = crate::server_fns::patch_account_inner(
        &pool,
        patch_account.id,
        AccountPatchFields {
            name: Some("Renamed account".into()),
            ..AccountPatchFields::default()
        },
    );
    let account_state = crate::server_fns::patch_account_inner(
        &pool,
        patch_account.id,
        AccountPatchFields {
            tone: Some("blue".into()),
            archived: Some(true),
            ..AccountPatchFields::default()
        },
    );
    let (account_name, account_state) = tokio::join!(account_name, account_state);
    account_name.into_anyhow()?;
    account_state.into_anyhow()?;
    let account = crate::server_fns::list_accounts_inner(&pool, Some(currency.id), true)
        .await
        .into_anyhow()?
        .into_iter()
        .find(|account| account.id == patch_account.id)
        .expect("patched account");
    assert_eq!(account.name, "Renamed account");
    assert_eq!(account.tone, "blue");
    assert!(account.archived);

    let patch_category = seed_category(&pool, currency.id, "Patch category").await?;
    let category_icon = crate::server_fns::patch_category_inner(
        &pool,
        patch_category.id,
        CategoryPatchFields {
            icon: Some("P".into()),
            ..CategoryPatchFields::default()
        },
    );
    let category_state = crate::server_fns::patch_category_inner(
        &pool,
        patch_category.id,
        CategoryPatchFields {
            sort_order: Some(42),
            archived: Some(true),
            ..CategoryPatchFields::default()
        },
    );
    let (category_icon, category_state) = tokio::join!(category_icon, category_state);
    category_icon.into_anyhow()?;
    category_state.into_anyhow()?;
    let category = crate::server_fns::list_categories_inner(&pool, currency.id, true)
        .await
        .into_anyhow()?
        .into_iter()
        .find(|category| category.id == patch_category.id)
        .expect("patched category");
    assert_eq!(category.icon, "P");
    assert_eq!(category.sort_order, 42);
    assert!(category.archived);

    let transaction_account =
        seed_account(&pool, currency.id, "Transaction account", 10_000).await?;
    let transaction_category = seed_category(&pool, currency.id, "Transaction category").await?;
    let now: i64 = sqlx::query_scalar("SELECT unixepoch()")
        .fetch_one(&pool)
        .await?;
    let transaction = crate::server_fns::add_txn_inner(
        &pool,
        utc(),
        AddTxnFields {
            currency_id: currency.id,
            merchant: "initial".into(),
            category_id: transaction_category.id,
            account_id: transaction_account.id,
            amount: amount(-100),
            tag: "exp".into(),
            note: None,
            occurred_at: now,
        },
    )
    .await
    .into_anyhow()?;
    let transaction_merchant = crate::server_fns::patch_txn_inner(
        &pool,
        utc(),
        transaction.id,
        TxnPatchFields {
            merchant: Some("concurrent merchant".into()),
            ..TxnPatchFields::default()
        },
    );
    let transaction_amount = crate::server_fns::patch_txn_inner(
        &pool,
        utc(),
        transaction.id,
        TxnPatchFields {
            amount: Some("3.00".into()),
            note: Some(Some("concurrent note".into())),
            ..TxnPatchFields::default()
        },
    );
    let (transaction_merchant, transaction_amount) =
        tokio::join!(transaction_merchant, transaction_amount);
    transaction_merchant.into_anyhow()?;
    transaction_amount.into_anyhow()?;
    let stored: (String, MinorAmount, Option<String>) =
        sqlx::query_as("SELECT merchant, amount, note FROM fin_txn WHERE id = ?1")
            .bind(transaction.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(stored.0, "concurrent merchant");
    assert_eq!(stored.1, amount(-300));
    assert_eq!(stored.2.as_deref(), Some("concurrent note"));
    assert_eq!(balance(&pool, transaction_account.id).await?, amount(9_700));
    Ok(())
}

#[tokio::test]
async fn failed_partial_patches_roll_back_every_field_and_balance() -> anyhow::Result<()> {
    let pool = pool().await?;
    let currency = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;

    let account = seed_account(&pool, currency.id, "Original account", 0).await?;
    seed_account(&pool, currency.id, "Taken account", 0).await?;
    let result = crate::server_fns::patch_account_inner(
        &pool,
        account.id,
        AccountPatchFields {
            name: Some("Taken account".into()),
            archived: Some(true),
            ..AccountPatchFields::default()
        },
    )
    .await;
    assert!(result.is_err());
    let stored_account: (String, bool) =
        sqlx::query_as("SELECT name, archived FROM fin_account WHERE id = ?1")
            .bind(account.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(stored_account, ("Original account".into(), false));

    let category = seed_category(&pool, currency.id, "Original category").await?;
    seed_category(&pool, currency.id, "Taken category").await?;
    let result = crate::server_fns::patch_category_inner(
        &pool,
        category.id,
        CategoryPatchFields {
            name: Some("Taken category".into()),
            sort_order: Some(99),
            archived: Some(true),
            ..CategoryPatchFields::default()
        },
    )
    .await;
    assert!(result.is_err());
    let stored_category: (String, i64, bool) =
        sqlx::query_as("SELECT name, sort_order, archived FROM fin_category WHERE id = ?1")
            .bind(category.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(stored_category, ("Original category".into(), 0, false));

    let cash = seed_account(&pool, currency.id, "Rollback cash", 10_000).await?;
    let bank = seed_account(&pool, currency.id, "Rollback bank", 5_000).await?;
    let expense = seed_category(&pool, currency.id, "Rollback expense").await?;
    let now: i64 = sqlx::query_scalar("SELECT unixepoch()")
        .fetch_one(&pool)
        .await?;
    let transaction = crate::server_fns::add_txn_inner(
        &pool,
        utc(),
        AddTxnFields {
            currency_id: currency.id,
            merchant: "before".into(),
            category_id: expense.id,
            account_id: cash.id,
            amount: amount(-100),
            tag: "exp".into(),
            note: None,
            occurred_at: now,
        },
    )
    .await
    .into_anyhow()?;
    sqlx::raw_sql(
        "CREATE TRIGGER fin_txn_reject_patch
         BEFORE UPDATE ON fin_txn
         WHEN NEW.merchant = 'force rollback'
         BEGIN
             SELECT RAISE(ABORT, 'forced patch failure');
         END;",
    )
    .execute(&pool)
    .await?;
    let result = crate::server_fns::patch_txn_inner(
        &pool,
        utc(),
        transaction.id,
        TxnPatchFields {
            merchant: Some("force rollback".into()),
            account_id: Some(bank.id),
            amount: Some("2.00".into()),
            note: Some(Some("must not persist".into())),
            ..TxnPatchFields::default()
        },
    )
    .await;
    assert!(result.is_err());
    let stored_transaction: (String, i64, MinorAmount, Option<String>) =
        sqlx::query_as("SELECT merchant, account_id, amount, note FROM fin_txn WHERE id = ?1")
            .bind(transaction.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        stored_transaction,
        ("before".into(), cash.id, amount(-100), None)
    );
    assert_eq!(balance(&pool, cash.id).await?, amount(9_900));
    assert_eq!(balance(&pool, bank.id).await?, amount(5_000));
    Ok(())
}

#[tokio::test]
async fn transaction_rejects_cross_currency_account_or_category() -> anyhow::Result<()> {
    let pool = pool().await?;
    let cny = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    let usd = crate::server_fns::create_currency_inner(
        &pool,
        "USD".into(),
        "$".into(),
        "US Dollar".into(),
        2,
        1,
    )
    .await
    .into_anyhow()?;
    let cny_account = seed_account(&pool, cny.id, "CNY Cash", 0).await?;
    let usd_account = seed_account(&pool, usd.id, "USD Cash", 0).await?;
    let cny_category = seed_category(&pool, cny.id, "Food").await?;
    let now: i64 = sqlx::query_scalar("SELECT unixepoch()")
        .fetch_one(&pool)
        .await?;

    let result = crate::server_fns::add_txn_inner(
        &pool,
        utc(),
        AddTxnFields {
            currency_id: cny.id,
            merchant: "Wrong account".into(),
            category_id: cny_category.id,
            account_id: usd_account.id,
            amount: amount(-100),
            tag: "exp".into(),
            note: None,
            occurred_at: now,
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(balance(&pool, cny_account.id).await?, amount(0));
    assert_eq!(balance(&pool, usd_account.id).await?, amount(0));
    Ok(())
}

#[tokio::test]
async fn transfer_is_a_module_owned_atomic_aggregate() -> anyhow::Result<()> {
    let pool = pool().await?;
    let cny = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    let usd = crate::server_fns::create_currency_inner(
        &pool,
        "USD".into(),
        "$".into(),
        "US Dollar".into(),
        2,
        1,
    )
    .await
    .into_anyhow()?;
    let from = seed_account(&pool, cny.id, "CNY Bank", 100_000).await?;
    let to = seed_account(&pool, usd.id, "USD Bank", 50_000).await?;
    let now: i64 = sqlx::query_scalar("SELECT unixepoch()")
        .fetch_one(&pool)
        .await?;

    let transfer = crate::server_fns::add_transfer_inner(
        &pool,
        utc(),
        AddTransferFields {
            from_account_id: from.id,
            to_account_id: to.id,
            from_amount: amount(7_250),
            to_amount: amount(1_000),
            note: Some("exchange".into()),
            occurred_at: now,
        },
    )
    .await
    .into_anyhow()?;
    assert!(transfer.id > 0);
    assert_eq!(balance(&pool, from.id).await?, amount(92_750));
    assert_eq!(balance(&pool, to.id).await?, amount(51_000));

    let legs: Vec<(String, String, MinorAmount)> = sqlx::query_as(
        "SELECT tag, transfer_role, amount FROM fin_txn WHERE transfer_id = ?1 ORDER BY transfer_role",
    )
    .bind(transfer.id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(legs.len(), 2);
    assert!(legs.iter().all(|leg| leg.0 == "tfr"));
    assert_eq!(legs.iter().filter(|leg| leg.1 == "out").count(), 1);
    assert_eq!(legs.iter().filter(|leg| leg.1 == "in").count(), 1);

    assert!(crate::server_fns::delete_transfer_inner(&pool, transfer.id)
        .await
        .into_anyhow()?);
    assert_eq!(balance(&pool, from.id).await?, amount(100_000));
    assert_eq!(balance(&pool, to.id).await?, amount(50_000));
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fin_txn WHERE transfer_id = ?1")
        .bind(transfer.id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(remaining, 0);
    Ok(())
}

#[tokio::test]
async fn deleting_either_transfer_leg_deletes_both_and_restores_balances() -> anyhow::Result<()> {
    let pool = pool().await?;
    let currency = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    let from = seed_account(&pool, currency.id, "From", 10_000).await?;
    let to = seed_account(&pool, currency.id, "To", 1_000).await?;
    let now: i64 = sqlx::query_scalar("SELECT unixepoch()")
        .fetch_one(&pool)
        .await?;
    let transfer = crate::server_fns::add_transfer_inner(
        &pool,
        utc(),
        AddTransferFields {
            from_account_id: from.id,
            to_account_id: to.id,
            from_amount: amount(2_500),
            to_amount: amount(2_500),
            note: None,
            occurred_at: now,
        },
    )
    .await
    .into_anyhow()?;
    let leg_id: i64 = sqlx::query_scalar("SELECT id FROM fin_txn WHERE transfer_id = ?1 LIMIT 1")
        .bind(transfer.id)
        .fetch_one(&pool)
        .await?;
    assert!(crate::server_fns::delete_txn_inner(&pool, leg_id)
        .await
        .into_anyhow()?);
    assert_eq!(balance(&pool, from.id).await?, amount(10_000));
    assert_eq!(balance(&pool, to.id).await?, amount(1_000));
    let transfer_exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_transfer WHERE id = ?1)")
            .bind(transfer.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(transfer_exists, 0);
    Ok(())
}

#[tokio::test]
async fn budget_and_month_reports_ignore_transfers() -> anyhow::Result<()> {
    let pool = pool().await?;
    let timezone = utc();
    let currency = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    let cash = seed_account(&pool, currency.id, "Cash", 0).await?;
    let bank = seed_account(&pool, currency.id, "Bank", 10_000).await?;
    let food = seed_category(&pool, currency.id, "Food").await?;
    let now: i64 = sqlx::query_scalar("SELECT unixepoch()")
        .fetch_one(&pool)
        .await?;
    let period = timezone.date(now).expect("current date").ym();

    crate::server_fns::add_txn_inner(
        &pool,
        timezone,
        AddTxnFields {
            currency_id: currency.id,
            merchant: "Salary".into(),
            category_id: food.id,
            account_id: cash.id,
            amount: amount(20_000),
            tag: "inc".into(),
            note: None,
            occurred_at: now,
        },
    )
    .await
    .into_anyhow()?;
    crate::server_fns::add_txn_inner(
        &pool,
        timezone,
        AddTxnFields {
            currency_id: currency.id,
            merchant: "Lunch".into(),
            category_id: food.id,
            account_id: cash.id,
            amount: amount(-3_000),
            tag: "exp".into(),
            note: None,
            occurred_at: now,
        },
    )
    .await
    .into_anyhow()?;
    crate::server_fns::add_transfer_inner(
        &pool,
        timezone,
        AddTransferFields {
            from_account_id: bank.id,
            to_account_id: cash.id,
            from_amount: amount(1_000),
            to_amount: amount(1_000),
            note: None,
            occurred_at: now,
        },
    )
    .await
    .into_anyhow()?;
    let budget =
        crate::server_fns::set_budget_inner(&pool, currency.id, &period, food.id, amount(5_000))
            .await
            .into_anyhow()?
            .expect("positive budget");
    assert_eq!(budget.used, amount(3_000));

    let summary = crate::server_fns::load_month_summary(&pool, timezone, now, currency.id)
        .await
        .into_anyhow()?;
    assert_eq!(summary.income, amount(20_000));
    assert_eq!(summary.expense, amount(3_000));
    assert_eq!(summary.savings, amount(17_000));
    assert_eq!(summary.transaction_count, 2);
    assert_eq!(summary.budget_total, amount(5_000));

    let months = crate::server_fns::load_month_buckets_12(&pool, timezone, now, currency.id)
        .await
        .into_anyhow()?;
    let current = months
        .iter()
        .find(|bucket| bucket.period == period)
        .unwrap();
    assert_eq!(current.income, amount(20_000));
    assert_eq!(current.expense, amount(3_000));
    assert_eq!(current.net, amount(17_000));
    Ok(())
}

#[test]
fn date_input_uses_the_selected_local_date_midpoint() -> anyhow::Result<()> {
    let kiritimati = timezone("Pacific/Kiritimati");
    let occurred_at = crate::server_fns::parse_occurred_at(kiritimati, "2026-07-14")
        .into_anyhow()?
        .expect("date timestamp");

    assert_eq!(
        kiritimati.date(occurred_at).expect("local date").ymd(),
        "2026-07-14"
    );
    assert_eq!(
        utc().date(occurred_at).expect("UTC date").ymd(),
        "2026-07-13",
        "the stored instant may be on the prior UTC day without changing the business date"
    );
    assert!(crate::server_fns::parse_occurred_at(timezone("Pacific/Apia"), "2011-12-30").is_err());
    Ok(())
}

#[tokio::test]
async fn persisted_business_date_and_month_do_not_drift_with_display_timezone() -> anyhow::Result<()>
{
    let pool = pool().await?;
    let shanghai = timezone("Asia/Shanghai");
    let los_angeles = timezone("America/Los_Angeles");
    let currency = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    let account = seed_account(&pool, currency.id, "Stable date account", 0).await?;
    let category = seed_category(&pool, currency.id, "Stable date expense").await?;
    let occurred_at = crate::server_fns::parse_occurred_at(shanghai, "2026-07-14")
        .into_anyhow()?
        .expect("date timestamp");
    assert_eq!(
        los_angeles
            .date(occurred_at)
            .expect("Los Angeles instant")
            .ymd(),
        "2026-07-13",
        "the raw instant intentionally falls on another display date"
    );

    let created = crate::server_fns::add_txn_inner(
        &pool,
        shanghai,
        AddTxnFields {
            currency_id: currency.id,
            merchant: "Shanghai purchase".into(),
            category_id: category.id,
            account_id: account.id,
            amount: amount(-1_400),
            tag: "exp".into(),
            note: None,
            occurred_at,
        },
    )
    .await
    .into_anyhow()?;
    assert_eq!(created.occurred_date, "2026-07-14");

    let updated = crate::server_fns::patch_txn_inner(
        &pool,
        los_angeles,
        created.id,
        TxnPatchFields {
            merchant: Some("Edited in Los Angeles".into()),
            occurred_at: Some(occurred_at),
            ..TxnPatchFields::default()
        },
    )
    .await
    .into_anyhow()?;
    assert_eq!(updated.occurred_date, "2026-07-14");
    assert_eq!(updated.occurred_at, occurred_at);

    let july_now = los_angeles
        .date_midpoint(ep_core::CalendarDate {
            year: 2026,
            month: 7,
            day: 20,
        })
        .expect("July timestamp");
    let data =
        crate::server_fns::load_finance_data_inner(&pool, los_angeles, july_now, currency.id)
            .await
            .into_anyhow()?;
    let listed = data
        .transactions
        .iter()
        .find(|transaction| transaction.id == created.id)
        .expect("created transaction is listed");
    assert_eq!(listed.occurred_date, "2026-07-14");
    assert_eq!(data.month.period, "2026-07");
    assert_eq!(data.month.expense, amount(1_400));
    assert_eq!(data.month.transaction_count, 1);
    assert_eq!(
        data.months_12
            .iter()
            .find(|month| month.period == "2026-07")
            .map(|month| month.expense),
        Some(amount(1_400))
    );

    let stored: String = sqlx::query_scalar("SELECT occurred_on FROM fin_txn WHERE id = ?1")
        .bind(created.id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(stored, "2026-07-14");

    let csv = crate::server_fns::export_csv_inner(&pool, los_angeles, currency.id)
        .await
        .into_anyhow()?;
    assert!(csv
        .lines()
        .nth(1)
        .is_some_and(|line| line.starts_with(&format!(
            "{},2026-07-14,2026-07-13T21:00:00-07:00,",
            created.id
        ))));
    Ok(())
}

#[tokio::test]
async fn month_reports_share_dst_aware_timezone_boundaries() -> anyhow::Result<()> {
    let pool = pool().await?;
    let timezone = timezone("America/New_York");
    let march = timezone.month_range(2024, 3).expect("March range");
    assert_eq!(
        march.end - march.start,
        31 * 86_400 - 3_600,
        "March 2024 contains the spring-forward transition"
    );
    let now = timezone
        .date_midpoint(ep_core::CalendarDate {
            year: 2024,
            month: 3,
            day: 15,
        })
        .expect("mid-March timestamp");
    let currency = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    let account = seed_account(&pool, currency.id, "Boundary account", 0).await?;
    let category = seed_category(&pool, currency.id, "Boundary expense").await?;

    let entries = [
        ("February edge", march.start - 1, -100, "2024-02-29"),
        ("March start", march.start, -200, "2024-03-01"),
        ("March end", march.end - 1, -300, "2024-03-31"),
        ("April edge", march.end, -400, "2024-04-01"),
    ];
    for (merchant, occurred_at, value, expected_date) in entries {
        let txn = crate::server_fns::add_txn_inner(
            &pool,
            timezone,
            AddTxnFields {
                currency_id: currency.id,
                merchant: merchant.into(),
                category_id: category.id,
                account_id: account.id,
                amount: amount(value),
                tag: "exp".into(),
                note: None,
                occurred_at,
            },
        )
        .await
        .into_anyhow()?;
        assert_eq!(txn.occurred_date, expected_date);
    }

    let budget = crate::server_fns::set_budget_inner(
        &pool,
        currency.id,
        &march.label,
        category.id,
        amount(1_000),
    )
    .await
    .into_anyhow()?
    .expect("positive budget");
    assert_eq!(budget.used, amount(500));

    let summary = crate::server_fns::load_month_summary(&pool, timezone, now, currency.id)
        .await
        .into_anyhow()?;
    assert_eq!(summary.period, "2024-03");
    assert_eq!(summary.expense, amount(500));
    assert_eq!(summary.transaction_count, 2);

    let months = crate::server_fns::load_month_buckets_12(&pool, timezone, now, currency.id)
        .await
        .into_anyhow()?;
    assert_eq!(months.len(), 12);
    assert_eq!(
        months.last().map(|month| month.period.as_str()),
        Some("2024-03")
    );
    assert_eq!(
        months
            .iter()
            .find(|month| month.period == "2024-02")
            .map(|month| month.expense),
        Some(amount(100))
    );
    assert_eq!(months.last().map(|month| month.expense), Some(amount(500)));
    Ok(())
}

#[tokio::test]
async fn csv_export_uses_integer_ids_and_neutralizes_formulas() -> anyhow::Result<()> {
    let pool = pool().await?;
    let timezone = timezone("Pacific/Kiritimati");
    let currency = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    let cash = seed_account(&pool, currency.id, "Cash", 0).await?;
    let misc = seed_category(&pool, currency.id, "Misc").await?;
    let occurred_at = crate::server_fns::parse_occurred_at(timezone, "2026-07-14")
        .into_anyhow()?
        .expect("date timestamp");
    let txn = crate::server_fns::add_txn_inner(
        &pool,
        timezone,
        AddTxnFields {
            currency_id: currency.id,
            merchant: "=HYPERLINK(\"bad\")".into(),
            category_id: misc.id,
            account_id: cash.id,
            amount: amount(-123),
            tag: "exp".into(),
            note: None,
            occurred_at,
        },
    )
    .await
    .into_anyhow()?;
    assert_eq!(txn.occurred_date, "2026-07-14");
    let csv = crate::server_fns::export_csv_inner(&pool, timezone, currency.id)
        .await
        .into_anyhow()?;
    assert_eq!(
        csv.lines().next(),
        Some(
            "id,occurred_on,occurred_at,merchant,category,account,currency,amount,type,note,transfer_id"
        )
    );
    assert!(csv.contains(&txn.id.to_string()));
    assert!(csv.contains(&format!("{},2026-07-14,", txn.id)));
    assert!(csv.contains("2026-07-14T12:00:00+14:00"));
    assert!(csv.contains("'=HYPERLINK"));
    Ok(())
}

#[tokio::test]
async fn reports_and_csv_are_not_truncated_with_the_recent_ledger_page() -> anyhow::Result<()> {
    let pool = pool().await?;
    let timezone = utc();
    let currency = crate::server_fns::resolve_currency(&pool, 0)
        .await
        .into_anyhow()?;
    let cash = seed_account(&pool, currency.id, "Long ledger", 0).await?;
    let category = seed_category(&pool, currency.id, "Frequent expense").await?;
    let now: i64 = sqlx::query_scalar("SELECT unixepoch()")
        .fetch_one(&pool)
        .await?;
    for index in 0..101 {
        crate::server_fns::add_txn_inner(
            &pool,
            timezone,
            AddTxnFields {
                currency_id: currency.id,
                merchant: format!("Expense {index}"),
                category_id: category.id,
                account_id: cash.id,
                amount: amount(-1),
                tag: "exp".into(),
                note: None,
                occurred_at: now,
            },
        )
        .await
        .into_anyhow()?;
    }

    let data = crate::server_fns::load_finance_data_inner(&pool, timezone, now, currency.id)
        .await
        .into_anyhow()?;
    assert_eq!(
        data.transactions.len(),
        100,
        "the visible ledger stays bounded"
    );
    assert_eq!(data.month.expense, amount(101));
    assert_eq!(data.category_summary.len(), 1);
    assert_eq!(data.category_summary[0].value, amount(101));

    let csv = crate::server_fns::export_csv_inner(&pool, timezone, currency.id)
        .await
        .into_anyhow()?;
    assert_eq!(csv.lines().count(), 102, "header plus all 101 transactions");
    Ok(())
}
