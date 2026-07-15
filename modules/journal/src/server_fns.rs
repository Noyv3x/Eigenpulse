#![cfg_attr(
    not(feature = "ssr"),
    allow(
        unused_variables,
        reason = "Leptos server-function parameters are serialized by client builds while implementations are SSR-only"
    )
)]

#[cfg(feature = "ssr")]
use crate::model::JournalEntryInput;
#[cfg(feature = "ssr")]
use crate::model::JournalPage;
use crate::model::{JournalAnalytics, JournalData, JournalEntry};
#[cfg(feature = "ssr")]
use crate::model::{JournalDayBucket, JournalMonthBucket, JournalTagBucket};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;

#[cfg(feature = "ssr")]
pub(crate) const JOURNAL_PAGE_SIZE: u32 = 20;
#[cfg(feature = "ssr")]
pub(crate) const JOURNAL_API_MAX_PAGE_SIZE: u32 = 100;

#[server(LoadJournal, "/api/_internal/journal", "Url", "load")]
pub async fn load_journal(
    query: String,
    include_archived: bool,
    offset: u32,
) -> Result<JournalData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let timezone = state.timezone();
        let now = ep_core::unix_now();
        let dates = JournalDateContext::from_snapshot(timezone, now)
            .ok_or_else(|| ep_core::server_err("invalid journal date context"))?;
        let page = list_entries_inner(
            &state.db,
            &query,
            include_archived,
            None,
            None,
            offset,
            JOURNAL_PAGE_SIZE,
        )
        .await
        .map_err(ep_core::server_err)?;
        Ok(JournalData {
            today: dates.today,
            entries: page.entries,
            next_offset: page.next_offset,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(LoadJournalAnalytics, "/api/_internal/journal", "Url", "analytics")]
pub async fn load_journal_analytics(
    include_archived: bool,
) -> Result<JournalAnalytics, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let timezone = state.timezone();
        let now = ep_core::unix_now();
        let dates = JournalDateContext::from_snapshot(timezone, now)
            .ok_or_else(|| ep_core::server_err("invalid journal date context"))?;
        journal_analytics_inner(&state.db, include_archived, &dates)
            .await
            .map_err(ep_core::server_err)
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    LoadJournalEntryForEdit,
    "/api/_internal/journal",
    "Url",
    "entry_for_edit"
)]
pub async fn load_entry_for_edit(id: i64) -> Result<JournalEntry, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        positive_id(id)?;
        let state = ep_auth::authed_state().await?;
        get_entry_inner(&state.db, id)
            .await
            .map_err(ep_core::server_err)?
            .ok_or_else(|| ep_i18n::err_with("journal.err.entry_not_found", id))
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    LoadJournalHomeSummary,
    "/api/_internal/journal",
    "Url",
    "home_summary"
)]
pub async fn load_home_summary() -> Result<ep_core::ModuleSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let timezone = state.timezone();
        let now = ep_core::unix_now();
        let dates = JournalDateContext::from_snapshot(timezone, now)
            .ok_or_else(|| ep_core::server_err("invalid journal date context"))?;
        let summary = summary_inner(&state.db, &dates.current_month)
            .await
            .map_err(ep_core::server_err)?;
        let months = journal_month_buckets_inner(&state.db, false, &dates)
            .await
            .map_err(ep_core::server_err)?;
        let trend = ep_core::normalize_summary_trend(
            "journal.chart.entries_monthly",
            months
                .into_iter()
                .rev()
                .take(8)
                .rev()
                .map(|month| (month.period, month.entries, month.entries.to_string())),
        );
        Ok(ep_core::ModuleSummary {
            slug: crate::DESCRIPTOR.slug.to_string(),
            state: if summary.active == 0 {
                ep_core::ModuleSummaryState::Empty
            } else {
                ep_core::ModuleSummaryState::Ready
            },
            metrics: vec![
                ep_core::SummaryMetric {
                    label_key: "journal.summary.entries".into(),
                    value: summary.active.to_string(),
                    detail: None,
                },
                ep_core::SummaryMetric {
                    label_key: "journal.summary.this_month".into(),
                    value: summary.this_month.to_string(),
                    detail: None,
                },
                ep_core::SummaryMetric {
                    label_key: "journal.summary.archived".into(),
                    value: summary.archived.to_string(),
                    detail: None,
                },
                ep_core::SummaryMetric {
                    label_key: "journal.summary.latest".into(),
                    value: summary.latest.unwrap_or_else(|| "—".into()),
                    detail: None,
                },
            ],
            trend,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(CreateJournalEntry, "/api/_internal/journal", "Url", "create")]
pub async fn create_entry(
    title: String,
    body: String,
    entry_date: String,
    mood: String,
    tags: String,
) -> Result<JournalEntry, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        create_entry_inner(
            &state.db,
            JournalEntryInput {
                title,
                body,
                entry_date,
                mood: optional_text(mood),
                tags,
            },
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(UpdateJournalEntry, "/api/_internal/journal", "Url", "update")]
pub async fn update_entry(
    id: i64,
    title: String,
    body: String,
    entry_date: String,
    mood: String,
    tags: String,
) -> Result<JournalEntry, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        update_entry_inner(
            &state.db,
            id,
            JournalEntryInput {
                title,
                body,
                entry_date,
                mood: optional_text(mood),
                tags,
            },
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(ArchiveJournalEntry, "/api/_internal/journal", "Url", "archive")]
pub async fn archive_entry(id: i64, archived: bool) -> Result<JournalEntry, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        archive_entry_inner(&state.db, id, archived).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(DeleteJournalEntry, "/api/_internal/journal", "Url", "delete")]
pub async fn delete_entry(id: i64) -> Result<ep_core::EntityId, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        if !delete_entry_inner(&state.db, id).await? {
            return Err(ep_i18n::err_with("journal.err.entry_not_found", id));
        }
        Ok(ep_core::EntityId::new(id))
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
#[derive(sqlx::FromRow)]
pub(crate) struct SummaryRow {
    pub(crate) active: i64,
    pub(crate) this_month: i64,
    pub(crate) archived: i64,
    pub(crate) latest: Option<String>,
}

#[cfg(feature = "ssr")]
pub(crate) async fn summary_inner(
    pool: &sqlx::SqlitePool,
    current_month: &str,
) -> Result<SummaryRow, sqlx::Error> {
    sqlx::query_as(
        "SELECT
            COUNT(*) FILTER (WHERE archived_at IS NULL) AS active,
            COUNT(*) FILTER (
                WHERE archived_at IS NULL
                  AND substr(entry_date, 1, 7) = ?1
            ) AS this_month,
            COUNT(*) FILTER (WHERE archived_at IS NOT NULL) AS archived,
            MAX(entry_date) FILTER (WHERE archived_at IS NULL) AS latest
         FROM jrn_entry",
    )
    .bind(current_month)
    .fetch_one(pool)
    .await
}

/// Calendar-only boundaries derived from one application timezone and clock
/// snapshot. Journal entry dates stay as `YYYY-MM-DD` values and are never
/// converted to instants.
#[cfg(feature = "ssr")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JournalDateContext {
    pub(crate) today: String,
    pub(crate) current_month: String,
    pub(crate) current_year: i32,
    pub(crate) previous_year: i32,
    pub(crate) recent_months: Vec<String>,
    pub(crate) recent_month_start: String,
    pub(crate) recent_month_end: String,
    pub(crate) calendar_start: String,
    pub(crate) calendar_end: String,
    pub(crate) trailing_365_start: String,
    pub(crate) trailing_365_end: String,
}

#[cfg(feature = "ssr")]
impl JournalDateContext {
    pub(crate) fn from_snapshot(timezone: ep_core::AppTimezone, now: i64) -> Option<Self> {
        let today = timezone.date(now)?;
        let previous_year = today.year.checked_sub(1)?;
        let next_year = today.year.checked_add(1)?;
        let recent_months = timezone
            .recent_months(now, 12)?
            .into_iter()
            .map(|range| range.label)
            .collect::<Vec<_>>();
        let recent_month_start = format!("{}-01", recent_months.first()?);
        let (end_year, end_month) = if today.month == 12 {
            (next_year, 1)
        } else {
            (today.year, today.month + 1)
        };
        let trailing_365_start = timezone.shift_date(today, -364)?.ymd();
        let trailing_365_end = timezone.shift_date(today, 1)?.ymd();

        Some(Self {
            today: today.ymd(),
            current_month: today.ym(),
            current_year: today.year,
            previous_year,
            recent_months,
            recent_month_start,
            recent_month_end: format!("{end_year:04}-{end_month:02}-01"),
            calendar_start: format!("{previous_year:04}-01-01"),
            calendar_end: format!("{next_year:04}-01-01"),
            trailing_365_start,
            trailing_365_end,
        })
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn journal_month_buckets_inner(
    pool: &sqlx::SqlitePool,
    include_archived: bool,
    dates: &JournalDateContext,
) -> Result<Vec<JournalMonthBucket>, sqlx::Error> {
    let rows = sqlx::query_as::<_, JournalMonthBucket>(
        "SELECT substr(entry_date, 1, 7) AS period, COUNT(*) AS entries
           FROM jrn_entry
          WHERE (?1 OR archived_at IS NULL)
            AND entry_date >= ?2 AND entry_date < ?3
          GROUP BY substr(entry_date, 1, 7)
          ORDER BY period",
    )
    .bind(include_archived)
    .bind(&dates.recent_month_start)
    .bind(&dates.recent_month_end)
    .fetch_all(pool)
    .await?;
    let mut counts = rows
        .into_iter()
        .map(|bucket| (bucket.period, bucket.entries))
        .collect::<std::collections::HashMap<_, _>>();
    Ok(dates
        .recent_months
        .iter()
        .map(|period| JournalMonthBucket {
            period: period.clone(),
            entries: counts.remove(period).unwrap_or(0),
        })
        .collect())
}

#[cfg(feature = "ssr")]
pub(crate) async fn journal_analytics_inner(
    pool: &sqlx::SqlitePool,
    include_archived: bool,
    dates: &JournalDateContext,
) -> Result<JournalAnalytics, sqlx::Error> {
    let months = journal_month_buckets_inner(pool, include_archived, dates).await?;
    let days = sqlx::query_as::<_, JournalDayBucket>(
        "SELECT entry_date, COUNT(*) AS entries
           FROM jrn_entry
          WHERE (?1 OR archived_at IS NULL)
            AND entry_date >= ?2 AND entry_date < ?3
          GROUP BY entry_date
          ORDER BY entry_date",
    )
    .bind(include_archived)
    .bind(&dates.calendar_start)
    .bind(&dates.calendar_end)
    .fetch_all(pool)
    .await?;
    let tag_rows: Vec<String> = sqlx::query_scalar(
        "SELECT tags FROM jrn_entry
          WHERE (?1 OR archived_at IS NULL)
            AND entry_date >= ?2 AND entry_date < ?3
            AND tags <> ''
          ORDER BY id",
    )
    .bind(include_archived)
    .bind(&dates.trailing_365_start)
    .bind(&dates.trailing_365_end)
    .fetch_all(pool)
    .await?;
    let tags = aggregate_top_tags(tag_rows);
    Ok(JournalAnalytics {
        current_year: dates.current_year,
        previous_year: dates.previous_year,
        months,
        days,
        tags,
    })
}

#[cfg(feature = "ssr")]
fn aggregate_top_tags(rows: Vec<String>) -> Vec<JournalTagBucket> {
    let mut counts = std::collections::HashMap::<String, (String, i64)>::new();
    for row in rows {
        let mut row_keys = std::collections::HashSet::<String>::new();
        for raw in row.split(',') {
            let tag = raw.trim();
            if tag.is_empty() {
                continue;
            }
            let key = tag_key(tag);
            if !row_keys.insert(key.clone()) {
                continue;
            }
            let entry = counts.entry(key).or_insert_with(|| (tag.to_string(), 0));
            entry.1 = entry.1.saturating_add(1);
        }
    }
    let mut tags = counts
        .into_iter()
        .map(|(key, (name, entries))| (key, name, entries))
        .collect::<Vec<_>>();
    tags.sort_by(|left, right| right.2.cmp(&left.2).then_with(|| left.0.cmp(&right.0)));
    let other = tags
        .get(8..)
        .unwrap_or_default()
        .iter()
        .fold(0_i64, |total, item| total.saturating_add(item.2));
    tags.truncate(8);
    let mut buckets = tags
        .into_iter()
        .map(|(_, name, entries)| JournalTagBucket {
            name,
            entries,
            is_other: false,
        })
        .collect::<Vec<_>>();
    if other > 0 {
        buckets.push(JournalTagBucket {
            name: String::new(),
            entries: other,
            is_other: true,
        });
    }
    buckets
}

#[cfg(feature = "ssr")]
fn tag_key(value: &str) -> String {
    value.to_lowercase()
}

#[cfg(feature = "ssr")]
pub(crate) async fn list_entries_inner(
    pool: &sqlx::SqlitePool,
    query: &str,
    include_archived: bool,
    date_from: Option<&str>,
    date_to: Option<&str>,
    offset: u32,
    limit: u32,
) -> Result<JournalPage, sqlx::Error> {
    let query = query.trim();
    let pattern = escaped_like_pattern(query);
    let limit = limit.clamp(1, JOURNAL_API_MAX_PAGE_SIZE);
    let fetch_limit = i64::from(limit) + 1;
    let mut entries = sqlx::query_as(
        "SELECT id, title,
                substr(body, 1, 400) AS body_preview,
                length(body) > 400 AS body_truncated,
                entry_date, mood, tags, archived_at
           FROM jrn_entry
          WHERE (?1 OR archived_at IS NULL)
            AND (?2 = ''
                OR title LIKE ?3 ESCAPE '!'
                OR body LIKE ?3 ESCAPE '!'
                OR tags LIKE ?3 ESCAPE '!'
                OR mood LIKE ?3 ESCAPE '!')
            AND (?4 IS NULL OR entry_date >= ?4)
            AND (?5 IS NULL OR entry_date <= ?5)
          ORDER BY entry_date DESC, id DESC
          LIMIT ?6 OFFSET ?7",
    )
    .bind(include_archived)
    .bind(query)
    .bind(pattern)
    .bind(date_from)
    .bind(date_to)
    .bind(fetch_limit)
    .bind(i64::from(offset))
    .fetch_all(pool)
    .await?;
    let has_more = entries.len() > limit as usize;
    entries.truncate(limit as usize);
    Ok(JournalPage {
        entries,
        next_offset: has_more.then(|| offset.saturating_add(limit)),
    })
}

#[cfg(feature = "ssr")]
fn escaped_like_pattern(query: &str) -> String {
    let mut pattern = String::with_capacity(query.len() + 2);
    pattern.push('%');
    for character in query.chars() {
        if matches!(character, '!' | '%' | '_') {
            pattern.push('!');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

#[cfg(feature = "ssr")]
pub(crate) async fn get_entry_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
) -> Result<Option<JournalEntry>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, title, body, entry_date, mood, tags, archived_at, created_at, updated_at
           FROM jrn_entry WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

#[cfg(feature = "ssr")]
pub(crate) async fn create_entry_inner(
    pool: &sqlx::SqlitePool,
    input: JournalEntryInput,
) -> Result<JournalEntry, ServerFnError> {
    let input = validate_input(input)?;
    sqlx::query_as(
        "INSERT INTO jrn_entry (title, body, entry_date, mood, tags)
         VALUES (?1, ?2, ?3, ?4, ?5)
         RETURNING id, title, body, entry_date, mood, tags, archived_at, created_at, updated_at",
    )
    .bind(input.title)
    .bind(input.body)
    .bind(input.entry_date)
    .bind(input.mood)
    .bind(input.tags)
    .fetch_one(pool)
    .await
    .map_err(ep_core::server_err)
}

#[cfg(feature = "ssr")]
pub(crate) async fn update_entry_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
    input: JournalEntryInput,
) -> Result<JournalEntry, ServerFnError> {
    positive_id(id)?;
    let input = validate_input(input)?;
    sqlx::query_as(
        "UPDATE jrn_entry
            SET title = ?2, body = ?3, entry_date = ?4, mood = ?5, tags = ?6,
                updated_at = unixepoch()
          WHERE id = ?1
         RETURNING id, title, body, entry_date, mood, tags, archived_at, created_at, updated_at",
    )
    .bind(id)
    .bind(input.title)
    .bind(input.body)
    .bind(input.entry_date)
    .bind(input.mood)
    .bind(input.tags)
    .fetch_optional(pool)
    .await
    .map_err(ep_core::server_err)?
    .ok_or_else(|| ep_i18n::err_with("journal.err.entry_not_found", id))
}

#[cfg(feature = "ssr")]
#[derive(Debug, Default)]
pub(crate) struct JournalEntryPatchInput {
    pub title: Option<String>,
    pub body: Option<String>,
    pub entry_date: Option<String>,
    pub mood: Option<Option<String>>,
    pub tags: Option<String>,
    pub archived: Option<bool>,
}

#[cfg(feature = "ssr")]
impl JournalEntryPatchInput {
    pub(crate) fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.body.is_none()
            && self.entry_date.is_none()
            && self.mood.is_none()
            && self.tags.is_none()
            && self.archived.is_none()
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn patch_entry_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
    patch: JournalEntryPatchInput,
) -> Result<bool, ServerFnError> {
    positive_id(id)?;
    let patch = validate_patch_input(patch)?;
    let title_present = patch.title.is_some();
    let body_present = patch.body.is_some();
    let entry_date_present = patch.entry_date.is_some();
    let mood_present = patch.mood.is_some();
    let tags_present = patch.tags.is_some();
    let archived_present = patch.archived.is_some();
    let mood = patch.mood.flatten();
    let result = sqlx::query(
        "UPDATE jrn_entry
            SET title = CASE WHEN ?2 THEN ?3 ELSE title END,
                body = CASE WHEN ?4 THEN ?5 ELSE body END,
                entry_date = CASE WHEN ?6 THEN ?7 ELSE entry_date END,
                mood = CASE WHEN ?8 THEN ?9 ELSE mood END,
                tags = CASE WHEN ?10 THEN ?11 ELSE tags END,
                archived_at = CASE
                    WHEN NOT ?12 THEN archived_at
                    WHEN ?13 THEN unixepoch()
                    ELSE NULL
                END,
                updated_at = unixepoch()
          WHERE id = ?1",
    )
    .bind(id)
    .bind(title_present)
    .bind(patch.title)
    .bind(body_present)
    .bind(patch.body)
    .bind(entry_date_present)
    .bind(patch.entry_date)
    .bind(mood_present)
    .bind(mood)
    .bind(tags_present)
    .bind(patch.tags)
    .bind(archived_present)
    .bind(patch.archived.unwrap_or(false))
    .execute(pool)
    .await
    .map_err(ep_core::server_err)?;
    Ok(result.rows_affected() == 1)
}

#[cfg(feature = "ssr")]
pub(crate) async fn archive_entry_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
    archived: bool,
) -> Result<JournalEntry, ServerFnError> {
    positive_id(id)?;
    sqlx::query_as(
        "UPDATE jrn_entry
            SET archived_at = CASE WHEN ?2 THEN unixepoch() ELSE NULL END,
                updated_at = unixepoch()
          WHERE id = ?1
         RETURNING id, title, body, entry_date, mood, tags, archived_at, created_at, updated_at",
    )
    .bind(id)
    .bind(archived)
    .fetch_optional(pool)
    .await
    .map_err(ep_core::server_err)?
    .ok_or_else(|| ep_i18n::err_with("journal.err.entry_not_found", id))
}

#[cfg(feature = "ssr")]
pub(crate) async fn delete_entry_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
) -> Result<bool, ServerFnError> {
    positive_id(id)?;
    let result = sqlx::query("DELETE FROM jrn_entry WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(ep_core::server_err)?;
    Ok(result.rows_affected() == 1)
}

#[cfg(feature = "ssr")]
fn validate_input(input: JournalEntryInput) -> Result<JournalEntryInput, ServerFnError> {
    let title = input.title.trim().to_string();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(ep_i18n::err("journal.err.title"));
    }
    if input.body.chars().count() > 100_000 {
        return Err(ep_i18n::err("journal.err.body"));
    }
    validate_date(&input.entry_date)?;
    let mood = input.mood.and_then(optional_text);
    if mood
        .as_ref()
        .is_some_and(|value| value.chars().count() > 40)
    {
        return Err(ep_i18n::err("journal.err.mood"));
    }
    let tags = normalize_tags(&input.tags)?;
    Ok(JournalEntryInput {
        title,
        body: input.body,
        entry_date: input.entry_date,
        mood,
        tags,
    })
}

#[cfg(feature = "ssr")]
fn validate_patch_input(
    patch: JournalEntryPatchInput,
) -> Result<JournalEntryPatchInput, ServerFnError> {
    if patch.is_empty() {
        return Err(ep_core::server_err("journal entry patch must not be empty"));
    }

    let title = patch
        .title
        .map(|title| {
            let title = title.trim().to_string();
            if title.is_empty() || title.chars().count() > 200 {
                return Err(ep_i18n::err("journal.err.title"));
            }
            Ok(title)
        })
        .transpose()?;
    if patch
        .body
        .as_ref()
        .is_some_and(|body| body.chars().count() > 100_000)
    {
        return Err(ep_i18n::err("journal.err.body"));
    }
    if let Some(entry_date) = patch.entry_date.as_deref() {
        validate_date(entry_date)?;
    }
    let mood = patch.mood.map(|mood| mood.and_then(optional_text));
    if mood
        .as_ref()
        .and_then(Option::as_ref)
        .is_some_and(|mood| mood.chars().count() > 40)
    {
        return Err(ep_i18n::err("journal.err.mood"));
    }
    let tags = patch.tags.map(|tags| normalize_tags(&tags)).transpose()?;

    Ok(JournalEntryPatchInput {
        title,
        body: patch.body,
        entry_date: patch.entry_date,
        mood,
        tags,
        archived: patch.archived,
    })
}

#[cfg(feature = "ssr")]
pub(crate) fn validate_date(value: &str) -> Result<(), ServerFnError> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err(ep_i18n::err("journal.err.date"));
    }
    let year: u32 = value[0..4]
        .parse()
        .map_err(|_| ep_i18n::err("journal.err.date"))?;
    let month: u32 = value[5..7]
        .parse()
        .map_err(|_| ep_i18n::err("journal.err.date"))?;
    let day: u32 = value[8..10]
        .parse()
        .map_err(|_| ep_i18n::err("journal.err.date"))?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year == 0 || day == 0 || day > max_day {
        return Err(ep_i18n::err("journal.err.date"));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
fn normalize_tags(value: &str) -> Result<String, ServerFnError> {
    let mut tags = Vec::<String>::new();
    let mut keys = std::collections::HashSet::<String>::new();
    for raw in value.split(',') {
        let tag = raw.trim().trim_start_matches('#').trim();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > 40 {
            return Err(ep_i18n::err("journal.err.tags"));
        }
        if !keys.insert(tag_key(tag)) {
            continue;
        }
        if tags.len() == 20 {
            return Err(ep_i18n::err("journal.err.tags"));
        }
        tags.push(tag.to_string());
    }
    let normalized = tags.join(", ");
    if normalized.chars().count() > 1_000 {
        return Err(ep_i18n::err("journal.err.tags"));
    }
    Ok(normalized)
}

#[cfg(feature = "ssr")]
pub(crate) fn optional_text(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(feature = "ssr")]
fn positive_id(id: i64) -> Result<(), ServerFnError> {
    if id > 0 {
        Ok(())
    } else {
        Err(ep_i18n::err("journal.err.id"))
    }
}
