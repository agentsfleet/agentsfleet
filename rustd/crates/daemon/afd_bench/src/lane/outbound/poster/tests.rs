//! What the poster answers, what it counts, and what it refuses to count.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::time::Duration;
use std::collections::BTreeMap;

use af_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_dragonfly::{EventId, OutboundDelivery};
use afd_outbound::retry::DELIVERY_ATTEMPTS;
use afd_outbound::{Deliver as _, Verdict};

use super::{Behaviour, Scripted};

/// A reply address the scripted poster never reads.
const REPLY_ADDRESS: &str = "{}";

/// The destination fleet scripted to answer at once.
const FAST: &str = "0195b4ba-8d3a-7000-8abc-000000000001";

/// The destination fleet scripted to refuse every attempt.
const REFUSES: &str = "0195b4ba-8d3a-7000-8abc-000000000002";

/// A destination fleet no script names.
const FOREIGN: &str = "0195b4ba-8d3a-7000-8abc-000000000009";

fn job(id: &str, destination: &str) -> OutboundDelivery {
    OutboundDelivery {
        id: EventId::of(id),
        provider: "slack".to_owned(),
        destination: REPLY_ADDRESS.to_owned(),
        workspace_id: mint_id(),
        fleet_id: Uuid7::parse(destination).expect("a canonical destination fleet"),
        event_id: mint_id(),
        answer: "a".to_owned(),
    }
}

fn poster() -> Scripted {
    let mut behaviours = BTreeMap::new();
    behaviours.insert(FAST.to_owned(), Behaviour::Fast);
    behaviours.insert(REFUSES.to_owned(), Behaviour::Retryable);
    Scripted::new(behaviours, Duration::ZERO, Duration::ZERO)
}

#[tokio::test]
async fn test_a_delivered_job_settles_and_a_refused_one_does_not_until_the_ladder_ends() {
    let poster = poster();

    assert_eq!(poster.deliver(&job("1-0", FAST)).await, Verdict::Delivered);
    assert_eq!(poster.settled(), 1);

    for attempt in 1..DELIVERY_ATTEMPTS {
        assert_eq!(
            poster.deliver(&job("2-0", REFUSES)).await,
            Verdict::Retryable
        );
        assert_eq!(
            poster.settled(),
            1,
            "attempt {attempt} of the ladder is not the end of it"
        );
    }
    poster.deliver(&job("2-0", REFUSES)).await;
    assert_eq!(
        poster.settled(),
        2,
        "the ladder's last attempt settles the job"
    );
}

#[tokio::test]
async fn test_a_job_this_run_did_not_queue_is_answered_but_never_counted() {
    let poster = poster();

    let verdict = poster.deliver(&job("9-0", FOREIGN)).await;

    assert_eq!(verdict, Verdict::Delivered, "it leaves the queue");
    assert_eq!(poster.settled(), 0, "it is not one of ours");
    let seen = poster.seen();
    assert!(seen.attempts().is_empty());
    assert_eq!(seen.foreign(), 1, "and the result says how many there were");
}

#[tokio::test]
async fn test_every_attempt_is_stamped_in_order() {
    let poster = poster();
    poster.deliver(&job("3-0", REFUSES)).await;
    poster.deliver(&job("3-0", REFUSES)).await;

    let seen = poster.seen();
    let attempts = seen.attempts().get("3-0").expect("two attempts on one job");
    assert_eq!(attempts.len(), 2);
    assert!(attempts[0].at <= attempts[1].at);
    assert_eq!(attempts[0].behaviour, Behaviour::Retryable);
}
