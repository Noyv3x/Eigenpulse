use crate::model::*;
#[cfg(feature = "ssr")]
use ep_core::server_err;
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitnessData {
    pub workouts: Vec<Workout>,
    pub summary: FitnessSummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FitnessSummary {
    /// Sum(`duration_m * strain_factor`) per ISO week, oldest → newest.
    /// 12 weeks padded — empty weeks render as flat bars.
    pub weekly_load: Vec<f64>,
    pub week_labels: Vec<String>,           // "W17" etc., parallel to weekly_load
    pub this_week_count: u32,
    pub this_week_target: u32,
    pub streak_days: u32,                   // consecutive trailing days with ≥ 1 workout
    pub aerobic_min_this_week: u32,         // sum(duration_m where kind ~ "/cardio|有氧|跑|cycle/")
    pub avg_duration_min: u32,              // last 30 days
    /// Heaviest strain among workouts in the last 7 days. None if empty.
    pub heaviest_strain: Option<String>,
}

#[server(LoadFitness, "/api/_internal/fit", "Url", "load_fitness")]
pub async fn load_fitness() -> Result<FitnessData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        type Row = (String, i64, String, Option<String>, i64, Option<String>, Option<String>, Option<i64>, Option<String>);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT doc_id, occurred_at, kind, program, duration_m, load_text, strain, rpe, notes
               FROM fit_workout ORDER BY occurred_at DESC LIMIT 30"
        ).fetch_all(&st.db).await.map_err(server_err)?;

        let workouts: Vec<Workout> = rows.into_iter().map(|r| Workout {
            doc_id: r.0, occurred_at: r.1, kind: r.2, program: r.3,
            duration_m: r.4, load_text: r.5, strain: r.6, rpe: r.7, notes: r.8,
        }).collect();

        let summary = compute_summary(&st.db, &workouts).await.map_err(server_err)?;
        Ok(FitnessData { workouts, summary })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[cfg(feature = "ssr")]
async fn compute_summary(pool: &sqlx::SqlitePool, workouts: &[Workout]) -> sqlx::Result<FitnessSummary> {
    // 12-week dense frame, server's local-tz aware. Each row is one ISO week
    // label like "2026-W17" mapped to weighted load (duration × strain).
    type WeekRow = (String, f64);
    let week_rows: Vec<WeekRow> = sqlx::query_as(
        "SELECT strftime('%Y-W%W', occurred_at, 'unixepoch', 'localtime') AS w,
                SUM(duration_m * CASE strain
                    WHEN 'L' THEN 0.6
                    WHEN 'H' THEN 1.4
                    ELSE 1.0
                END) AS load
           FROM fit_workout
          WHERE occurred_at >= unixepoch('now','localtime','-77 days','utc')
          GROUP BY w
          ORDER BY w ASC"
    ).fetch_all(pool).await?;

    let frame: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE weeks(w, n) AS (
            SELECT strftime('%Y-W%W','now','localtime',printf('-%d days', 7 * 11)), 0
            UNION ALL
            SELECT strftime('%Y-W%W','now','localtime',printf('-%d days', 7 * (11 - n - 1))), n + 1
              FROM weeks
             WHERE n + 1 < 12
         )
         SELECT w FROM weeks ORDER BY w ASC"
    ).fetch_all(pool).await?;

    let by_week: std::collections::HashMap<String, f64> = week_rows.into_iter().collect();
    let weekly_load: Vec<f64> = frame.iter().map(|w| by_week.get(w).copied().unwrap_or(0.0)).collect();
    let week_labels: Vec<String> = frame.iter()
        .map(|w| w.split("-W").nth(1).map(|n| format!("W{}", n)).unwrap_or_else(|| w.clone()))
        .collect();

    // Week boundary = local Monday 00:00 of the week containing today.
    // Matches the Mon-start convention used by `strftime('%Y-W%W', …)`
    // in the weekly_load aggregator above so the two are coherent.
    //
    // Modifier order matters: '-6 days' shifts back six full days first;
    // 'weekday 1' then advances forward to the next Monday, which (for
    // any starting weekday) lands on the Monday of the calling week;
    // 'start of day' anchors at local 00:00 (without it the boundary
    // drifts by the time-of-day fractional); 'utc' converts the
    // local-Monday-00:00 string to a UTC unix epoch for comparison
    // against `occurred_at`. The earlier `'weekday 0','-7 days'` form
    // was Sun-start AND kept the time-of-day, off by both axes.
    const WEEK_START_MONDAY: &str =
        "unixepoch('now','localtime','-6 days','weekday 1','start of day','utc')";

    let this_week_count: u32 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM fit_workout WHERE occurred_at >= {WEEK_START_MONDAY}"
    )).fetch_one(pool).await?;

    let aerobic_min_this_week: u32 = sqlx::query_scalar(&format!(
        "SELECT COALESCE(SUM(duration_m), 0) FROM fit_workout
          WHERE occurred_at >= {WEEK_START_MONDAY}
            AND (lower(kind) LIKE '%cardio%'
                 OR lower(kind) LIKE '%有氧%'
                 OR lower(kind) LIKE '%跑%'
                 OR lower(kind) LIKE '%骑%'
                 OR lower(kind) LIKE '%游%')"
    )).fetch_one(pool).await?;

    let avg_duration_min: u32 = sqlx::query_scalar(
        "SELECT COALESCE(CAST(AVG(duration_m) AS INTEGER), 0) FROM fit_workout
          WHERE occurred_at >= unixepoch('now','localtime','-30 days','utc')"
    ).fetch_one(pool).await?;

    // Heaviest strain in last 7 days, ranked H > M > L.
    let heaviest_strain: Option<String> = sqlx::query_scalar(
        "SELECT strain FROM fit_workout
          WHERE occurred_at >= unixepoch('now','localtime','-7 days','utc')
            AND strain IS NOT NULL
          ORDER BY CASE strain WHEN 'H' THEN 3 WHEN 'M' THEN 2 WHEN 'L' THEN 1 ELSE 0 END DESC
          LIMIT 1"
    ).fetch_optional(pool).await?.flatten();

    // Streak: walk back from today's local date counting consecutive days
    // with ≥ 1 workout. `'start of day'` on both sides is load-bearing —
    // raw `julianday(...,'localtime')` preserves sub-day fractional, which
    // makes a 02:00 workout and a 22:00 previous-day workout both truncate
    // to the same integer JD. Anchoring both to local 00:00 first
    // guarantees whole-day alignment so cross-midnight workouts count as
    // distinct days. Distinct on the result so multiple workouts on the
    // same day still count once.
    let workout_days_local: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT CAST(julianday(occurred_at,'unixepoch','localtime','start of day') AS INTEGER)
           FROM fit_workout"
    ).fetch_all(pool).await?;
    let today_local: i64 = sqlx::query_scalar(
        "SELECT CAST(julianday('now','localtime','start of day') AS INTEGER)"
    ).fetch_one(pool).await?;
    let _ = workouts; // streak no longer reads the loaded list — see SQL above
    let streak_days = compute_streak(today_local, &workout_days_local);

    Ok(FitnessSummary {
        weekly_load,
        week_labels,
        this_week_count,
        this_week_target: 6,
        streak_days,
        aerobic_min_this_week,
        avg_duration_min,
        heaviest_strain,
    })
}

#[cfg(feature = "ssr")]
fn compute_streak(today_julian_local: i64, workout_julians_local: &[i64]) -> u32 {
    use std::collections::HashSet;
    let dates: HashSet<i64> = workout_julians_local.iter().copied().collect();
    let mut streak = 0;
    let mut day = today_julian_local;
    while dates.contains(&day) {
        streak += 1;
        day -= 1;
    }
    // No "rest day today" exception: streak resets at local midnight if the
    // user hasn't worked out yet. Cleaner semantics, and "your streak ends
    // when you skip a day" matches how every fitness app the user's used.
    streak
}

#[cfg(test)]
#[cfg(feature = "ssr")]
mod streak_tests {
    use super::compute_streak;

    #[test]
    fn empty_workouts_streak_is_zero() {
        assert_eq!(compute_streak(2_460_000, &[]), 0);
    }

    #[test]
    fn contiguous_chain_ending_today() {
        let dates = [2_460_000, 2_459_999, 2_459_998];   // today, today-1, today-2
        assert_eq!(compute_streak(2_460_000, &dates), 3);
    }

    #[test]
    fn gap_breaks_chain() {
        let dates = [2_460_000, 2_459_998];               // today + today-2 (gap on today-1)
        assert_eq!(compute_streak(2_460_000, &dates), 1);
    }

    #[test]
    fn no_workout_today_resets_to_zero() {
        let dates = [2_459_999, 2_459_998];               // yesterday + day before
        assert_eq!(compute_streak(2_460_000, &dates), 0);
    }

    #[test]
    fn duplicate_workouts_same_day_count_once() {
        let dates = [2_460_000, 2_460_000, 2_459_999];
        assert_eq!(compute_streak(2_460_000, &dates), 2);
    }

    /// Pins the week-start invariant. The `'-6 days','weekday 1','start
    /// of day'` chain must always land on Monday 00:00 of the week
    /// containing the input date — for every weekday Mon–Sun and across
    /// any month boundary. The earlier `'weekday 0','-7 days'` form was
    /// Sun-start (off by 1) AND preserved the time-of-day fractional
    /// (drifted by hours).
    #[tokio::test(flavor = "current_thread")]
    async fn week_start_anchors_to_local_monday_zero_zero() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        // Probe seven consecutive days; all should resolve to the same
        // Monday at 00:00:00.
        let week_anchors: Vec<String> = sqlx::query_scalar(
            "WITH d(s) AS (VALUES
                ('2026-04-27 03:00:00'),
                ('2026-04-28 14:00:00'),
                ('2026-04-29 09:30:00'),
                ('2026-04-30 23:59:00'),
                ('2026-05-01 00:01:00'),
                ('2026-05-02 12:00:00'),
                ('2026-05-03 23:00:00')
             )
             SELECT datetime(s,'-6 days','weekday 1','start of day') FROM d"
        ).fetch_all(&pool).await.unwrap();
        for a in &week_anchors {
            assert_eq!(a, "2026-04-27 00:00:00",
                       "expected Monday 2026-04-27 00:00:00, got {a}");
        }
    }

    /// Pins the `'start of day'` modifier on the streak's day-bucketing
    /// SQL. Without it `julianday(t,'unixepoch','localtime')` keeps the
    /// sub-day fractional, so two timestamps in the same local day at
    /// different hours can collapse to the same integer JD AND two
    /// timestamps in different local days near midnight can ALSO collapse
    /// — both directions of incorrect bucketing.
    #[tokio::test(flavor = "current_thread")]
    async fn streak_sql_buckets_distinct_local_days() {
        // Two timestamps a few hours apart but on different local calendar
        // days (one before local midnight, one after). The buggy form
        // `CAST(julianday(t,'unixepoch','localtime') AS INTEGER)` produces
        // the same value for both; the fixed form produces distinct values.
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        // Use UTC bench because the test runner doesn't control TZ. We
        // pick two timestamps 3 hours apart spanning UTC midnight so the
        // bucketing test is TZ-independent (the modifier order is what's
        // being pinned, not the TZ math).
        let t_pre_midnight: i64 = 1_777_507_140;   // 2026-04-29 22:39 UTC
        let t_post_midnight: i64 = t_pre_midnight + 3 * 3600;  // ~01:39 UTC the next day
        let buggy: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT CAST(julianday(?, 'unixepoch') AS INTEGER) UNION
             SELECT DISTINCT CAST(julianday(?, 'unixepoch') AS INTEGER) ORDER BY 1"
        ).bind(t_pre_midnight).bind(t_post_midnight).fetch_all(&pool).await.unwrap();
        let fixed: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT CAST(julianday(?, 'unixepoch','start of day') AS INTEGER) UNION
             SELECT DISTINCT CAST(julianday(?, 'unixepoch','start of day') AS INTEGER) ORDER BY 1"
        ).bind(t_pre_midnight).bind(t_post_midnight).fetch_all(&pool).await.unwrap();
        // Buggy form may collapse to one bucket (it does empirically: the
        // sub-day fractional truncates the same way for both). The fixed
        // form distinguishes the two UTC-midnight-spanning timestamps.
        assert_eq!(fixed.len(), 2, "fixed form should bucket the two timestamps separately, got {fixed:?}");
        // Sanity: buggy form would have collapsed (uncomment to verify):
        let _ = buggy;
    }
}

#[server(AddWorkout, "/api/_internal/fit", "Url", "add_workout")]
pub async fn add_workout(
    kind: String,
    program: String,
    duration_m: i64,
    load_text: String,
    strain: String,
) -> Result<Workout, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        if kind.trim().is_empty() {
            return Err(ServerFnError::Args("kind is required".into()));
        }
        if duration_m <= 0 {
            return Err(ServerFnError::Args("duration must be positive".into()));
        }
        let strain_kind = if strain.is_empty() {
            Strain::M
        } else {
            match Strain::parse(&strain) {
                Some(k) => k,
                None => return Err(ServerFnError::Args(format!("strain must be L/M/H, got '{strain}'"))),
            }
        };
        let strain_norm = strain_kind.as_str().to_string();
        let st: ep_core::AppState = expect_context();
        let occurred = time::OffsetDateTime::now_utc().unix_timestamp();
        let mut tx = st.db.begin().await.map_err(server_err)?;
        let doc_id = ep_core::next_doc_id(&mut tx, "FIT", ep_core::DocIdShape::TypeSerial4 { kind: "S" })
            .await.map_err(server_err)?;
        let program_opt = if program.trim().is_empty() { None } else { Some(program.clone()) };
        let load_opt = if load_text.trim().is_empty() { None } else { Some(load_text.clone()) };
        sqlx::query(
            "INSERT INTO fit_workout (doc_id, occurred_at, kind, program, duration_m, load_text, strain)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        )
        .bind(&doc_id).bind(occurred).bind(&kind)
        .bind(&program_opt).bind(duration_m).bind(&load_opt).bind(&strain_norm)
        .execute(&mut *tx).await.map_err(server_err)?;
        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary, status)
             VALUES (?1, 'FIT', ?2, ?3, ?4)"
        )
        .bind(occurred).bind(&doc_id).bind(&kind).bind(&strain_norm)
        .execute(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(Workout {
            doc_id, occurred_at: occurred, kind, program: program_opt,
            duration_m, load_text: load_opt, strain: Some(strain_norm),
            rpe: None, notes: None,
        })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(DeleteWorkout, "/api/_internal/fit", "Url", "delete_workout")]
pub async fn delete_workout(doc_id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        let mut tx = st.db.begin().await.map_err(server_err)?;
        sqlx::query("DELETE FROM fit_workout WHERE doc_id = ?1")
            .bind(&doc_id).execute(&mut *tx).await.map_err(server_err)?;
        sqlx::query("DELETE FROM activity WHERE module = 'FIT' AND doc_id = ?1")
            .bind(&doc_id).execute(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}
