use crate::model::JournalEntryInput;
use crate::server_fns::{
    archive_entry_inner, create_entry_inner, delete_entry_inner, get_entry_inner,
    journal_analytics_inner, journal_month_buckets_inner, list_entries_inner, patch_entry_inner,
    summary_inner, update_entry_inner, validate_date, JournalDateContext, JournalEntryPatchInput,
};

const TEST_NOW: i64 = 1_767_227_400; // 2026-01-01 00:30:00Z

fn test_dates() -> JournalDateContext {
    JournalDateContext::from_snapshot(ep_core::AppTimezone::utc(), TEST_NOW)
        .expect("fixed test date context")
}

pub(crate) async fn migrated_pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
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
    .await
    .unwrap();
    ep_core::run_module_migrations(&pool, crate::MODULE)
        .await
        .expect("journal migrations");
    pool
}

#[tokio::test]
async fn concurrent_partial_patches_preserve_every_omitted_field() {
    let pool = migrated_pool().await;
    let created = create_entry_inner(
        &pool,
        input("Original", "Original body", "2026-07-12", "original"),
    )
    .await
    .unwrap();

    let title_patch = patch_entry_inner(
        &pool,
        created.id,
        JournalEntryPatchInput {
            title: Some("Revised title".into()),
            ..Default::default()
        },
    );
    let body_patch = patch_entry_inner(
        &pool,
        created.id,
        JournalEntryPatchInput {
            body: Some("Revised body".into()),
            ..Default::default()
        },
    );
    let archive_patch = patch_entry_inner(
        &pool,
        created.id,
        JournalEntryPatchInput {
            archived: Some(true),
            ..Default::default()
        },
    );
    let (title_result, body_result, archive_result) =
        tokio::join!(title_patch, body_patch, archive_patch);
    assert!(title_result.unwrap());
    assert!(body_result.unwrap());
    assert!(archive_result.unwrap());

    let patched = get_entry_inner(&pool, created.id).await.unwrap().unwrap();
    assert_eq!(patched.title, "Revised title");
    assert_eq!(patched.body, "Revised body");
    assert_eq!(patched.entry_date, "2026-07-12");
    assert_eq!(patched.mood.as_deref(), Some("calm"));
    assert_eq!(patched.tags, "original");
    assert!(patched.archived_at.is_some());
}

fn input(title: &str, body: &str, date: &str, tags: &str) -> JournalEntryInput {
    JournalEntryInput {
        title: title.into(),
        body: body.into(),
        entry_date: date.into(),
        mood: Some("calm".into()),
        tags: tags.into(),
    }
}

#[tokio::test]
async fn create_update_search_archive_and_delete_entry() {
    let pool = migrated_pool().await;
    let created = create_entry_inner(
        &pool,
        input(
            " First entry ",
            "A quiet morning",
            "2026-07-12",
            "life, #Rust, LIFE",
        ),
    )
    .await
    .unwrap();
    assert!(created.id > 0);
    assert_eq!(created.title, "First entry");
    assert_eq!(created.tags, "life, Rust");
    assert!(created.archived_at.is_none());

    let by_body = list_entries_inner(&pool, "quiet", false, None, None, 0, 20)
        .await
        .unwrap();
    assert_eq!(by_body.entries.len(), 1);
    assert_eq!(by_body.entries[0].id, created.id);
    assert_eq!(by_body.entries[0].body_preview, created.body);
    let outside_range = list_entries_inner(
        &pool,
        "",
        false,
        Some("2026-07-13"),
        Some("2026-07-31"),
        0,
        20,
    )
    .await
    .unwrap();
    assert!(outside_range.entries.is_empty());

    let updated = update_entry_inner(
        &pool,
        created.id,
        input("Updated", "Longer reflection", "2026-07-11", "reflection"),
    )
    .await
    .unwrap();
    assert_eq!(updated.title, "Updated");
    assert_eq!(updated.entry_date, "2026-07-11");

    let archived = archive_entry_inner(&pool, created.id, true).await.unwrap();
    assert!(archived.archived_at.is_some());
    assert!(list_entries_inner(&pool, "", false, None, None, 0, 20)
        .await
        .unwrap()
        .entries
        .is_empty());
    assert_eq!(
        list_entries_inner(&pool, "reflection", true, None, None, 0, 20)
            .await
            .unwrap()
            .entries
            .len(),
        1
    );

    let restored = archive_entry_inner(&pool, created.id, false).await.unwrap();
    assert!(restored.archived_at.is_none());
    assert!(delete_entry_inner(&pool, created.id).await.unwrap());
    assert!(get_entry_inner(&pool, created.id).await.unwrap().is_none());
    assert!(!delete_entry_inner(&pool, created.id).await.unwrap());
}

#[tokio::test]
async fn validation_rejects_invalid_dates_and_oversized_fields() {
    let pool = migrated_pool().await;
    for invalid in ["", "2026-02-29", "2024-13-01", "2024-04-31"] {
        assert!(validate_date(invalid).is_err(), "accepted {invalid}");
    }
    assert!(validate_date("2024-02-29").is_ok());

    let invalid = create_entry_inner(&pool, input(" ", "body", "2026-07-12", "tag")).await;
    assert!(invalid.is_err());

    let too_many_tags = (0..21)
        .map(|index| format!("tag{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let invalid =
        create_entry_inner(&pool, input("title", "body", "2026-07-12", &too_many_tags)).await;
    assert!(invalid.is_err());

    let twenty_tags = (0..20)
        .map(|index| format!("tag{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let duplicate_after_limit = format!("{twenty_tags},#TAG0");
    let accepted = create_entry_inner(
        &pool,
        input("title", "body", "2026-07-12", &duplicate_after_limit),
    )
    .await
    .expect("a duplicate must not count as a twenty-first tag");
    assert_eq!(accepted.tags.split(", ").count(), 20);

    let unicode_duplicate =
        create_entry_inner(&pool, input("unicode", "body", "2026-07-12", "É, é"))
            .await
            .expect("Unicode case variants must be treated as one tag");
    assert_eq!(unicode_duplicate.tags, "É");
}

#[tokio::test]
async fn list_is_paginated_and_never_returns_complete_large_bodies() {
    let pool = migrated_pool().await;
    let large_body = "x".repeat(2_000);
    for index in 0..25 {
        create_entry_inner(
            &pool,
            input(
                &format!("Entry {index}"),
                &large_body,
                "2026-07-12",
                "pagination",
            ),
        )
        .await
        .unwrap();
    }

    let first = list_entries_inner(&pool, "", false, None, None, 0, 20)
        .await
        .unwrap();
    assert_eq!(first.entries.len(), 20);
    assert_eq!(first.next_offset, Some(20));
    assert!(first
        .entries
        .iter()
        .all(|entry| { entry.body_preview.chars().count() == 400 && entry.body_truncated }));

    let second = list_entries_inner(&pool, "", false, None, None, 20, 20)
        .await
        .unwrap();
    assert_eq!(second.entries.len(), 5);
    assert_eq!(second.next_offset, None);
    assert!(first.entries.last().unwrap().id > second.entries[0].id);
}

#[tokio::test]
async fn keyword_search_treats_like_wildcards_as_plain_text() {
    let pool = migrated_pool().await;
    let percent = create_entry_inner(
        &pool,
        input("Progress 100%", "literal percent", "2026-07-12", "plain"),
    )
    .await
    .unwrap();
    let underscore = create_entry_inner(
        &pool,
        input("under_score", "literal underscore", "2026-07-12", "plain"),
    )
    .await
    .unwrap();
    create_entry_inner(
        &pool,
        input("ordinary", "no wildcard", "2026-07-12", "plain"),
    )
    .await
    .unwrap();

    let percent_matches = list_entries_inner(&pool, "%", false, None, None, 0, 20)
        .await
        .unwrap();
    assert_eq!(
        percent_matches
            .entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        [percent.id]
    );
    let underscore_matches = list_entries_inner(&pool, "_", false, None, None, 0, 20)
        .await
        .unwrap();
    assert_eq!(
        underscore_matches
            .entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        [underscore.id]
    );
}

#[tokio::test]
async fn analytics_are_zero_filled_and_respect_archive_visibility() {
    let pool = migrated_pool().await;
    let dates = test_dates();
    let today = dates.today.clone();
    let current_month = dates.current_month.clone();

    create_entry_inner(&pool, input("Today", "Body", &today, "Rust, health"))
        .await
        .unwrap();
    create_entry_inner(&pool, input("Another today", "Body", &today, "rust, focus"))
        .await
        .unwrap();
    let archived = create_entry_inner(&pool, input("Private", "Body", &today, "private"))
        .await
        .unwrap();
    archive_entry_inner(&pool, archived.id, true).await.unwrap();

    let active_months = journal_month_buckets_inner(&pool, false, &dates)
        .await
        .unwrap();
    assert_eq!(active_months.len(), 12);
    assert_eq!(active_months.last().unwrap().period, current_month);
    assert_eq!(active_months.last().unwrap().entries, 2);

    let active = journal_analytics_inner(&pool, false, &dates).await.unwrap();
    assert_eq!(active.months, active_months);
    assert_eq!(
        active
            .days
            .iter()
            .find(|day| day.entry_date == today)
            .map(|day| day.entries),
        Some(2)
    );
    let rust = active
        .tags
        .iter()
        .find(|tag| tag.name.eq_ignore_ascii_case("rust"))
        .unwrap();
    assert_eq!(rust.entries, 2);
    assert!(!active.tags.iter().any(|tag| tag.name == "private"));

    let with_archived = journal_analytics_inner(&pool, true, &dates).await.unwrap();
    assert_eq!(with_archived.months.last().unwrap().entries, 3);
    assert_eq!(
        with_archived
            .days
            .iter()
            .find(|day| day.entry_date == today)
            .map(|day| day.entries),
        Some(3)
    );
    assert!(with_archived
        .tags
        .iter()
        .any(|tag| tag.name == "private" && tag.entries == 1));
}

#[tokio::test]
async fn tag_analytics_keep_the_top_eight_and_fold_the_remainder() {
    let pool = migrated_pool().await;
    let dates = test_dates();
    let today = dates.today.clone();
    for index in 0..10 {
        create_entry_inner(
            &pool,
            input(
                &format!("Entry {index}"),
                "Body",
                &today,
                &format!("tag{index}"),
            ),
        )
        .await
        .unwrap();
    }

    let analytics = journal_analytics_inner(&pool, false, &dates).await.unwrap();
    assert_eq!(analytics.tags.len(), 9);
    assert_eq!(
        analytics.tags.last(),
        Some(&crate::model::JournalTagBucket {
            name: String::new(),
            entries: 2,
            is_other: true,
        })
    );
}

#[tokio::test]
async fn tag_analytics_exclude_future_entries_and_count_each_entry_once() {
    let pool = migrated_pool().await;
    let dates = test_dates();
    let today = dates.today.clone();
    let duplicate = create_entry_inner(&pool, input("Duplicate", "Body", &today, "É"))
        .await
        .unwrap();
    // Be defensive about rows created by older builds or direct database
    // maintenance: popularity is the number of entries, not occurrences.
    sqlx::query("UPDATE jrn_entry SET tags = 'É, é' WHERE id = ?1")
        .bind(duplicate.id)
        .execute(&pool)
        .await
        .unwrap();
    create_entry_inner(&pool, input("Future", "Body", "2099-01-01", "future-only"))
        .await
        .unwrap();

    let analytics = journal_analytics_inner(&pool, false, &dates).await.unwrap();
    let unicode = analytics
        .tags
        .iter()
        .find(|tag| tag.name.to_lowercase() == "é")
        .unwrap();
    assert_eq!(unicode.entries, 1);
    assert!(!analytics.tags.iter().any(|tag| tag.name == "future-only"));
}

#[test]
fn date_context_uses_one_timezone_snapshot_for_all_civil_boundaries() {
    let utc = test_dates();
    assert_eq!(utc.today, "2026-01-01");
    assert_eq!(utc.current_month, "2026-01");
    assert_eq!(utc.current_year, 2026);
    assert_eq!(utc.previous_year, 2025);
    assert_eq!(
        utc.recent_months.first().map(String::as_str),
        Some("2025-02")
    );
    assert_eq!(
        utc.recent_months.last().map(String::as_str),
        Some("2026-01")
    );
    assert_eq!(utc.recent_month_start, "2025-02-01");
    assert_eq!(utc.recent_month_end, "2026-02-01");
    assert_eq!(utc.calendar_start, "2025-01-01");
    assert_eq!(utc.calendar_end, "2027-01-01");
    assert_eq!(utc.trailing_365_start, "2025-01-02");
    assert_eq!(utc.trailing_365_end, "2026-01-02");

    let los_angeles = JournalDateContext::from_snapshot(
        ep_core::AppTimezone::parse("America/Los_Angeles").unwrap(),
        TEST_NOW,
    )
    .unwrap();
    assert_eq!(los_angeles.today, "2025-12-31");
    assert_eq!(los_angeles.current_month, "2025-12");
    assert_eq!(los_angeles.current_year, 2025);
    assert_eq!(los_angeles.recent_month_start, "2025-01-01");
    assert_eq!(los_angeles.recent_month_end, "2026-01-01");
    assert_eq!(los_angeles.trailing_365_start, "2025-01-01");
    assert_eq!(los_angeles.trailing_365_end, "2026-01-01");
}

#[tokio::test]
async fn summary_uses_the_supplied_local_month_without_shifting_entry_dates() {
    let pool = migrated_pool().await;
    create_entry_inner(&pool, input("December", "Body", "2025-12-31", "month"))
        .await
        .unwrap();
    create_entry_inner(
        &pool,
        input("Earlier December", "Body", "2025-12-01", "month"),
    )
    .await
    .unwrap();
    create_entry_inner(&pool, input("January", "Body", "2026-01-01", "month"))
        .await
        .unwrap();

    let utc = test_dates();
    let los_angeles = JournalDateContext::from_snapshot(
        ep_core::AppTimezone::parse("America/Los_Angeles").unwrap(),
        TEST_NOW,
    )
    .unwrap();
    let utc_summary = summary_inner(&pool, &utc.current_month).await.unwrap();
    let los_angeles_summary = summary_inner(&pool, &los_angeles.current_month)
        .await
        .unwrap();

    assert_eq!(utc_summary.this_month, 1);
    assert_eq!(los_angeles_summary.this_month, 2);
    assert_eq!(utc_summary.latest.as_deref(), Some("2026-01-01"));
    assert_eq!(los_angeles_summary.latest.as_deref(), Some("2026-01-01"));
}
