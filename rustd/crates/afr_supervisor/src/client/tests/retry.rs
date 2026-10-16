//! Idempotent calls retry transient errors; refusals stop immediately.
use crate::{
    client::{Verb, retrying},
    error,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test(start_paused = true)]
async fn a_blip_is_retried_and_a_refusal_is_not() {
    let blips = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&blips);
    let recovered = retrying(|| {
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
    let refused = retrying(|| {
        refused_count.fetch_add(1, Ordering::SeqCst);
        async { Err::<(), _>(error::refused(Verb::Hydrate, 403, None)) }
    })
    .await;
    let exhausted =
        retrying(|| async { Err::<(), _>(error::unavailable(Verb::Report, 502)) }).await;

    assert_eq!(recovered.unwrap(), 2);
    assert_eq!(
        refusals.load(Ordering::SeqCst),
        1,
        "a 4xx is final on its first answer"
    );
    assert!(!refused.unwrap_err().is_retryable());
    assert!(exhausted.unwrap_err().is_retryable());
}
