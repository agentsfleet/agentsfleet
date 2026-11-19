//! Idempotent calls retry transient errors; refusals stop immediately.
use crate::{client::Verb, error};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test(start_paused = true)]
async fn selected_daemon_attempt_limit_bounds_the_actual_retry_loop() {
    let settings = garde::Unvalidated::new(crate::config::RuntimeSettings {
        daemon_max_attempts: 2,
        daemon_initial_backoff_ms: 1,
        daemon_max_backoff_ms: 1,
        ..crate::config::RuntimeSettings::default()
    })
    .validate()
    .unwrap();
    let (plane, _) = crate::test_support::plane(|_| {
        crate::test_support::json(&crate::test_support::healthy().to(None))
    });
    let plane = plane.with_runtime(crate::config::RuntimePolicy::new(&settings));
    let sends = AtomicUsize::new(0);
    let result: crate::Result<()> = plane
        .retrying(|| {
            sends.fetch_add(1, Ordering::SeqCst);
            async { Err(error::unavailable(Verb::Hydrate, 503)) }
        })
        .await;
    assert!(result.unwrap_err().is_retryable());
    assert_eq!(sends.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn a_blip_is_retried_and_a_refusal_is_not() {
    let (plane, _) = crate::test_support::plane(|_| {
        crate::test_support::json(&crate::test_support::healthy().to(None))
    });
    let blips = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&blips);
    let recovered = plane
        .retrying(|| {
            let attempt = counted.fetch_add(1, Ordering::SeqCst);
            async move {
                if attempt < 2 {
                    Err(error::unavailable(Verb::Hydrate, 503))
                } else {
                    Ok(attempt)
                }
            }
        })
        .await;
    let refusals = Arc::new(AtomicUsize::new(0));
    let refused_count = Arc::clone(&refusals);
    let refused = plane
        .retrying(|| {
            refused_count.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(error::refused(Verb::Hydrate, 403, None)) }
        })
        .await;
    let exhausted = plane
        .retrying(|| async { Err::<(), _>(error::unavailable(Verb::Report, 502)) })
        .await;

    assert_eq!(recovered.unwrap(), 2);
    assert_eq!(
        refusals.load(Ordering::SeqCst),
        1,
        "a 4xx is final on its first answer"
    );
    assert!(!refused.unwrap_err().is_retryable());
    assert!(exhausted.unwrap_err().is_retryable());
}
