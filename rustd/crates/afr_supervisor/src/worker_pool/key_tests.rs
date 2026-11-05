//! The poll attempt's `Idempotency-Key` across retries and answers.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use afd_wire::runner::HeartbeatStatus;
use tokio::sync::watch;

use super::super::serve;
use super::{assigned, idle};
use crate::client::Verb;
use crate::error;
use crate::heartbeat::Assignment;
use crate::test_support::{Answer, Behaviour, FakeAgent, FakeEngine, Rig, daemon};

/// A poll whose answer never arrived is repeated under the same key, so the
/// daemon can free a lease it issued; any decoded answer — no work, or a
/// refusal — ends the attempt, and the next poll mints a fresh key.
#[tokio::test(start_paused = true)]
async fn a_poll_key_is_kept_until_an_answer_is_decoded() {
    let keys = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&keys);
    let polled = AtomicUsize::new(0);
    let answer = daemon(move |call| {
        (call.verb == Verb::Lease).then(|| {
            seen.lock()
                .unwrap()
                .push(call.key.as_ref().map(|key| key.as_str().to_owned()));
            match polled.fetch_add(1, Ordering::SeqCst) {
                0 | 1 => Answer::Fail(error::unavailable(Verb::Lease, 503)),
                2 => idle(Some(0)),
                3 => Answer::Fail(error::refused(Verb::Lease, 409, None)),
                _ => idle(Some(500)),
            }
        })
    });
    let rig = Rig::new(
        answer,
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    );
    let (published, watching) = watch::channel(assigned(1));
    let pool = tokio::spawn(serve(Arc::clone(&rig.lessee), watching));

    while keys.lock().unwrap().len() < 5 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    published.send_replace(Assignment {
        status: HeartbeatStatus::Drain,
        ..assigned(1)
    });
    drop(published);
    pool.await.unwrap();

    let keys = keys.lock().unwrap().clone();
    let key = |index: usize| keys.get(index).cloned().flatten().unwrap();
    assert_eq!(
        (
            key(0) == key(1),
            key(1) == key(2),
            key(2) != key(3),
            key(3) != key(4)
        ),
        (true, true, true, true),
        "kept across two 503s, ended by a decoded answer and by a refusal: {keys:?}"
    );
}
