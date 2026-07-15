use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::rngs::OsRng;
use tokio::sync::Semaphore;

/// Argon2 intentionally consumes roughly 19 MiB per operation. Keep login,
/// password-change, and bootstrap hashing from multiplying that cost without
/// bound in the 256 MiB container.
static ARGON_PERMITS: Semaphore = Semaphore::const_new(2);

fn hasher() -> anyhow::Result<Argon2<'static>> {
    let params = Params::new(19_456, 2, 1, None)
        .map_err(|e| anyhow::anyhow!("invalid argon2 params: {e}"))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

pub fn hash_password(plain: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = hasher()?
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("argon2 hash failed: {e}"))?
        .to_string();
    Ok(hash)
}

pub(crate) fn verify_password(plain: &str, encoded: &str) -> anyhow::Result<bool> {
    let parsed =
        PasswordHash::new(encoded).map_err(|e| anyhow::anyhow!("argon2 parse failed: {e}"))?;
    Ok(hasher()?.verify_password(plain.as_bytes(), &parsed).is_ok())
}

/// Async wrapper for `hash_password` that bounces the ~150 ms Argon2id
/// computation onto the blocking pool. Use from server fns / axum handlers
/// so the leptos runtime / tower worker isn't parked.
pub async fn hash_password_async(plain: String) -> anyhow::Result<String> {
    run_limited_blocking(&ARGON_PERMITS, move || hash_password(&plain)).await
}

/// Async wrapper for `verify_password`. Same rationale as `hash_password_async`.
pub async fn verify_password_async(plain: String, encoded: String) -> anyhow::Result<bool> {
    run_limited_blocking(&ARGON_PERMITS, move || verify_password(&plain, &encoded)).await
}

/// The permit deliberately moves into the blocking closure. Dropping or
/// aborting the async caller only detaches a `spawn_blocking` job; it cannot
/// stop the CPU/memory-heavy work. Keeping the permit in the outer future
/// would therefore release capacity early and let disconnected clients exceed
/// the process-wide Argon2 concurrency bound.
async fn run_limited_blocking<T, F>(limiter: &'static Semaphore, work: F) -> anyhow::Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
{
    let permit = limiter
        .acquire()
        .await
        .map_err(|_| anyhow::anyhow!("argon2 concurrency limiter closed"))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_verify_sync_roundtrip() {
        let h = hash_password("hunter2_test").unwrap();
        assert!(verify_password("hunter2_test", &h).unwrap());
        assert!(!verify_password("wrong", &h).unwrap());
    }

    #[test]
    fn hash_produces_distinct_outputs_for_same_input() {
        // SaltString::generate(OsRng) means two hashes of the same plaintext
        // must differ (otherwise we have a salt collision or a bug).
        let a = hash_password("same-plain").unwrap();
        let b = hash_password("same-plain").unwrap();
        assert_ne!(
            a, b,
            "same plaintext must produce different hashes (random salt)"
        );
        assert!(verify_password("same-plain", &a).unwrap());
        assert!(verify_password("same-plain", &b).unwrap());
    }

    #[test]
    fn verify_rejects_garbage_hash() {
        assert!(verify_password("anything", "not-a-real-hash").is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn hash_verify_async_roundtrip() {
        let h = hash_password_async("test1234".into()).await.unwrap();
        assert!(verify_password_async("test1234".into(), h.clone())
            .await
            .unwrap());
        assert!(!verify_password_async("nope".into(), h).await.unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_caller_keeps_permit_until_blocking_work_finishes() {
        use std::sync::mpsc;
        use std::time::Duration;
        use tokio::sync::oneshot;

        let limiter: &'static Semaphore = Box::leak(Box::new(Semaphore::new(1)));
        let (first_started_tx, first_started_rx) = oneshot::channel();
        let (release_first_tx, release_first_rx) = mpsc::channel();
        let first = tokio::spawn(run_limited_blocking(limiter, move || {
            let _ = first_started_tx.send(());
            release_first_rx.recv().expect("release first blocking job");
            Ok(())
        }));
        first_started_rx.await.expect("first job started");

        // Cancelling the async wrapper drops its JoinHandle, but
        // `spawn_blocking` keeps executing. Its permit must stay occupied.
        first.abort();
        let _ = first.await;

        let (second_started_tx, mut second_started_rx) = oneshot::channel();
        let second = tokio::spawn(run_limited_blocking(limiter, move || {
            let _ = second_started_tx.send(());
            Ok(())
        }));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut second_started_rx)
                .await
                .is_err(),
            "a cancelled caller must not release a still-running blocking job's permit"
        );

        release_first_tx.send(()).expect("release first job");
        tokio::time::timeout(Duration::from_secs(5), &mut second_started_rx)
            .await
            .expect("second job should start after the first exits")
            .expect("second start signal");
        second.await.expect("second wrapper joined").unwrap();
    }
}
