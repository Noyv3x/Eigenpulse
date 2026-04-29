// One-shot password reset. Builds against ep-auth's own hash_password so the
// Argon2id params match bootstrap exactly. The password is read from stdin
// (one line, trailing newline stripped) so it never appears in argv / shell
// history / `ps -ef`.
//
// Interactive — read into a shell var with echo off, then feed via here-string
// (the literal never lands in argv):
//
//   read -rs NEW_PW
//   cargo run -p ep-auth --features ssr --example reset_password <<< "$NEW_PW"
//   unset NEW_PW
//
// Optional EP_NEW_PASSWORD env var is honoured for non-TTY one-liners; prefer
// the here-string flow when possible since env vars are visible to root via
// /proc/<pid>/environ.

use std::env;
use std::io::{self, Read};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let new_pw = match env::var("EP_NEW_PASSWORD") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            // Take the first line only — anything past the first `\n` is
            // either accidental input or an attempt at multi-line passwords
            // we don't support. trim_end_matches would silently swallow
            // multiple trailing newlines as one password.
            buf.lines().next().unwrap_or("").to_string()
        }
    };
    let pw_len = new_pw.chars().count();
    if pw_len < 6 {
        anyhow::bail!("password must be at least 6 characters (got {})", pw_len);
    }
    let db_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://data/eigenpulse.db?mode=rwc".to_string());

    // Reuse the same connect options (WAL, busy_timeout, foreign_keys=ON) as
    // the main app pool — otherwise this connection would race the live
    // server with the default 0ms busy_timeout and fail with SQLITE_BUSY
    // the moment anything else holds a write lock.
    let pool = ep_db::open_pool(&db_url).await?;

    let hash = ep_auth::hash_password(&new_pw)?;

    // Hash + session purge run in one transaction so the new credential
    // never coexists with stale session rows. Without this, a crash between
    // the UPDATE and DELETE would leave previously issued cookies usable
    // against the new password.
    let mut tx = pool.begin().await?;
    let updated = sqlx::query("UPDATE app_user SET password_hash = ?1 WHERE id = 1")
        .bind(&hash)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if updated == 0 {
        anyhow::bail!("no row with id=1 in app_user — has the app ever been bootstrapped?");
    }
    ep_auth::purge_all_sessions(&mut *tx).await?;
    tx.commit().await?;
    eprintln!("password updated; all sessions invalidated");
    Ok(())
}
