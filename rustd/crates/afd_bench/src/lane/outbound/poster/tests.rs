//! What the poster answers, what it counts, and what it refuses to count.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::time::Duration;
use std::collections::BTreeMap;

use afd_outbound::retry::DELIVERY_ATTEMPTS;
use afd_outbound::{Deliver as _, Verdict};
use afd_redis::{EventId, OutboundDelivery};

use super::{Behaviour, Scripted};

fn job(id: &str, destination: &str) -> OutboundDelivery {
    OutboundDelivery {
        id: EventId::of(id),
        provider: "slack".to_owned(),
        workspace_id: "w".to_owned(),
        fleet_id: destination.to_owned(),
        event_id: "e".to_owned(),
        answer: "a".to_owned(),
    }
}

fn poster() -> Scripted {
    let mut behaviours = BTreeMap::new();
    behaviours.insert("fast".to_owned(), Behaviour::Fast);
    behaviours.insert("refuses".to_owned(), Behaviour::Retryable);
    Scripted::new(behaviours, Duration::ZERO, Duration::ZERO)
}

#[tokio::test]
async fn test_a_delivered_job_settles_and_a_refused_one_does_not_until_the_ladder_ends() {
    let poster = poster();

    assert_eq!(
        poster.deliver(&job("1-0", "fast")).await,
        Verdict::Delivered
    );
    assert_eq!(poster.settled(), 1);

    for attempt in 1..DELIVERY_ATTEMPTS {
        assert_eq!(
            poster.deliver(&job("2-0", "refuses")).await,
            Verdict::Retryable
        );
        assert_eq!(
            poster.settled(),
            1,
            "attempt {attempt} of the ladder is not the end of it"
        );
    }
    poster.deliver(&job("2-0", "refuses")).await;
    assert_eq!(
        poster.settled(),
        2,
        "the ladder's last attempt settles the job"
    );
}

#[tokio::test]
async fn test_a_job_this_run_did_not_queue_is_answered_but_never_counted() {
    let poster = poster();

    let verdict = poster
        .deliver(&job("9-0", "somebody-elses-destination"))
        .await;

    assert_eq!(verdict, Verdict::Delivered, "it leaves the queue");
    assert_eq!(poster.settled(), 0, "it is not one of ours");
    let seen = poster.seen();
    assert!(seen.attempts().is_empty());
    assert_eq!(seen.foreign(), 1, "and the result says how many there were");
}

#[tokio::test]
async fn test_every_attempt_is_stamped_in_order() {
    let poster = poster();
    poster.deliver(&job("3-0", "refuses")).await;
    poster.deliver(&job("3-0", "refuses")).await;

    let seen = poster.seen();
    let attempts = seen.attempts().get("3-0").expect("two attempts on one job");
    assert_eq!(attempts.len(), 2);
    assert!(attempts[0].at <= attempts[1].at);
    assert_eq!(attempts[0].behaviour, Behaviour::Retryable);
}
