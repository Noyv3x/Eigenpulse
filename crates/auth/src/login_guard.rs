//! Login-handler hardening primitives: an in-memory brute-force rate limiter
//! and double-submit-cookie CSRF helpers. Both are server-side only and depend
//! on nothing beyond `std`, `tokio::sync`, and the crate's existing `rand`.
//!
//! These are deliberately mechanism-only: the app layer owns the wiring (where
//! to store the [`LoginThrottle`], how to key it, and how to thread the CSRF
//! token through the login GET/POST). See the doc comments on each item for the
//! intended integration.

use rand::RngCore;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Fixed-window brute-force limiter for the login POST.
///
/// Policy (the defaults from [`LoginThrottle::default`]): at most
/// `max_attempts` failed login attempts are allowed per `window` per key. The
/// key is caller-chosen — the login handler keys by client IP. On the
/// `max_attempts + 1`-th attempt inside the window, [`check_and_record`] returns
/// `Err(RetryAfter)` carrying the seconds until the current window rolls over;
/// the handler should reject the attempt (HTTP 429 / a "try again later"
/// message) without ever running the expensive Argon2 verify.
///
/// This is a *fixed* window: the counter and window start reset the first time
/// a key is seen after its previous window elapsed. It is intentionally simple
/// (a `Mutex<HashMap>`, no background eviction) — the keyspace is bounded by the
/// number of distinct client IPs hitting a single-user NAS app, and stale
/// entries are overwritten lazily on the next attempt from the same key.
///
/// Only *failed* attempts should be recorded. The expected wiring is: call
/// [`check_and_record`] before verifying the password; on a **successful**
/// login, call [`reset`] to clear the key so a legitimate user isn't penalized
/// for earlier typos.
///
/// [`check_and_record`]: LoginThrottle::check_and_record
/// [`reset`]: LoginThrottle::reset
#[derive(Debug)]
pub struct LoginThrottle {
    max_attempts: u32,
    window: Duration,
    entries: Mutex<HashMap<String, Window>>,
}

#[derive(Debug, Clone, Copy)]
struct Window {
    attempts: u32,
    start: Instant,
}

/// Returned when a key has exhausted its attempts for the current window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryAfter {
    /// Seconds until the current window rolls over and attempts are allowed
    /// again. Suitable for a `Retry-After` header.
    pub seconds: u64,
}

impl Default for LoginThrottle {
    /// 5 failed attempts per 15-minute window per key.
    fn default() -> Self {
        Self::new(5, Duration::from_secs(15 * 60))
    }
}

impl LoginThrottle {
    /// Build a limiter allowing `max_attempts` failures per `window` per key.
    pub fn new(max_attempts: u32, window: Duration) -> Self {
        Self {
            max_attempts,
            window,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Record an attempt for `key` and report whether it is allowed.
    ///
    /// Returns `Ok(())` while the key is under its limit for the current
    /// window (and counts the attempt), or `Err(RetryAfter)` once the limit is
    /// exceeded. Call this *before* the password verify and only count failed
    /// attempts (see [`reset`](Self::reset) for the success path).
    pub async fn check_and_record(&self, key: &str) -> Result<(), RetryAfter> {
        self.check_and_record_at(key, Instant::now()).await
    }

    /// Testable core of [`check_and_record`](Self::check_and_record) with an
    /// injectable clock.
    async fn check_and_record_at(&self, key: &str, now: Instant) -> Result<(), RetryAfter> {
        let mut map = self.entries.lock().await;
        let entry = map.entry(key.to_string()).or_insert(Window {
            attempts: 0,
            start: now,
        });
        // Roll the window over if it has elapsed.
        if now.duration_since(entry.start) >= self.window {
            entry.attempts = 0;
            entry.start = now;
        }
        if entry.attempts >= self.max_attempts {
            let elapsed = now.duration_since(entry.start);
            let remaining = self.window.saturating_sub(elapsed);
            return Err(RetryAfter {
                seconds: remaining.as_secs().max(1),
            });
        }
        entry.attempts += 1;
        Ok(())
    }

    /// Clear a key's counter — call on successful login so earlier failed
    /// attempts don't count against a now-authenticated user.
    pub async fn reset(&self, key: &str) {
        self.entries.lock().await.remove(key);
    }
}

/// Mint a fresh, URL-safe CSRF token. Same entropy source and base62 charset as
/// the PAT generator (192 bits of randomness), so it is safe to embed in a
/// cookie value and a hidden form field without escaping.
pub fn issue_csrf_token() -> String {
    let mut buf = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut buf);
    crate::pat::base62_encode(&buf)
}

/// Constant-time-ish equality check for the double-submit-cookie CSRF defense.
///
/// Intended wiring (the app layer owns it):
/// 1. The login **GET** calls [`issue_csrf_token`], sets it as a cookie
///    (path `/`, `HttpOnly` is fine, `SameSite=Lax`, mirror
///    [`crate::session::cookie_secure`]), and embeds the same value in a hidden
///    `<input name="csrf">` in the login form.
/// 2. The login **POST** reads both the cookie value and the form field and
///    calls `verify_csrf(form_token, cookie_token)`. A `false` result (or a
///    missing cookie/field) rejects the request before any auth work.
///
/// Comparison is length-checked then byte-folded so it does not short-circuit
/// on the first differing byte, avoiding a timing oracle on the token.
pub fn verify_csrf(form_token: &str, cookie_token: &str) -> bool {
    let a = form_token.as_bytes();
    let b = cookie_token.as_bytes();
    if a.is_empty() || a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn throttle_blocks_after_max_attempts_within_window() {
        let throttle = LoginThrottle::new(3, Duration::from_secs(60));
        let key = "10.0.0.1";
        let t0 = Instant::now();

        // First 3 attempts allowed.
        for _ in 0..3 {
            assert!(throttle.check_and_record_at(key, t0).await.is_ok());
        }
        // 4th within the window is blocked, with a positive retry-after.
        let err = throttle
            .check_and_record_at(key, t0)
            .await
            .expect_err("should be throttled");
        assert!(err.seconds >= 1 && err.seconds <= 60);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn throttle_window_rolls_over() {
        let window = Duration::from_secs(60);
        let throttle = LoginThrottle::new(2, window);
        let key = "10.0.0.2";
        let t0 = Instant::now();

        assert!(throttle.check_and_record_at(key, t0).await.is_ok());
        assert!(throttle.check_and_record_at(key, t0).await.is_ok());
        assert!(throttle.check_and_record_at(key, t0).await.is_err());

        // After the window elapses the counter resets.
        let later = t0 + window + Duration::from_secs(1);
        assert!(throttle.check_and_record_at(key, later).await.is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn throttle_keys_are_independent_and_reset_clears() {
        let throttle = LoginThrottle::new(1, Duration::from_secs(60));
        let t0 = Instant::now();

        assert!(throttle.check_and_record_at("a", t0).await.is_ok());
        assert!(throttle.check_and_record_at("a", t0).await.is_err());
        // Different key is unaffected.
        assert!(throttle.check_and_record_at("b", t0).await.is_ok());
        // Reset clears the exhausted key.
        throttle.reset("a").await;
        assert!(throttle.check_and_record_at("a", t0).await.is_ok());
    }

    #[test]
    fn csrf_token_is_nonempty_and_alphanumeric() {
        let token = issue_csrf_token();
        assert!(!token.is_empty());
        assert!(token.bytes().all(|b| b.is_ascii_alphanumeric()));
    }

    #[test]
    fn verify_csrf_matches_only_identical_tokens() {
        let token = issue_csrf_token();
        assert!(verify_csrf(&token, &token));
        assert!(!verify_csrf(&token, &issue_csrf_token()));
        // Empty tokens never match (missing cookie/field guard).
        assert!(!verify_csrf("", ""));
        assert!(!verify_csrf(&token, ""));
        // Length mismatch is rejected without comparison.
        assert!(!verify_csrf("abc", "abcd"));
    }
}
