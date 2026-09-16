//! What a clone of the lanes shares with the original.
//!
//! Its own binary rather than a case inside `lanes.rs`, deliberately. That
//! suite's ceiling proof measures how many deliveries are in flight at once
//! and waits on a high-water mark; every additional `multi_thread` test in the
//! same binary competes for the same worker threads, and the ceiling test has
//! already been seen to exhaust its patience on a loaded runner. This claim
//! needs no concurrency at all, so it does not belong in that contention set.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::streams::EventId;
use afd_dragonfly::{Dragonfly, OutboundDelivery, OutboundQueue};
use afd_outbound::{Lanes, Posters};
use tokio_util::sync::CancellationToken;

#[path = "support/gated_poster.rs"]
#[allow(
    dead_code,
    reason = "shared support: the ceiling suite reads the high-water mark, this one does not"
)]
mod gated_poster;
#[path = "support/hanging_queue.rs"]
#[allow(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#[allow(
    dead_code,
    reason = "shared support: each suite drives a different part of the fake"
)]
mod hanging_queue;
#[path = "support/no_ledger.rs"]
mod no_ledger;

use self::gated_poster::Gated;
use self::hanging_queue::HangingQueue;

const PROVIDER: &str = "slack";
const FLEET_ID: &str = "0199a0b0-0000-7000-8000-0000000000f1";
const WORKSPACE: &str = "fast";
/// The deadline the client gives the fake for any one command.
const REQUEST_DEADLINE: Duration = Duration::from_secs(2);
/// How long the acknowledgement is waited for. Nothing here is held shut, so
/// reaching this means the lanes stopped rather than that they were slow.
const PATIENCE: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(5);

/// Cloning names the SAME lanes, not a second set. A component that took a
/// clone and opened its own lane map would deliver past the ceiling and
/// acknowledge through a tracker nobody drains.
#[tokio::test(flavor = "multi_thread")]
async fn a_clone_names_the_same_lanes() {
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();

    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(REQUEST_DEADLINE);
    let redis = Dragonfly::connect(&config)
        .await
        .expect("the fake queue answers a ping");
    let poster = Gated::default();
    let lanes = Lanes::new(
        Posters {
            slack: poster.clone(),
        },
        OutboundQueue::new(redis),
        no_ledger::no_ledger(),
        token.clone(),
    );
    let second_holder = lanes.clone();

    lanes
        .dispatch(Box::new(OutboundDelivery {
            id: EventId::of("1700000000001-0"),
            provider: PROVIDER.to_owned(),
            workspace_id: WORKSPACE.to_owned(),
            fleet_id: FLEET_ID.to_owned(),
            event_id: "shared-1".to_owned(),
            answer: "the run finished".to_owned(),
        }))
        .await;

    tokio::time::timeout(PATIENCE, async {
        while server.acks().is_empty() {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .expect("the answer dispatched through one holder is acknowledged");

    // Read through the OTHER holder: one lane map, so both see the same work.
    assert_eq!(second_holder.active(), lanes.active());

    token.cancel();
    second_holder.drain().await;
}

/// A delivery whose acknowledgement cannot be recorded is not retried here.
///
/// The destination already took the answer. What failed is the RECORD of it,
/// so the entry stays pending and the next process delivers it a second time —
/// which is why the whole path is at-least-once and why the destination's own
/// thread, not this ledger, is what a person reads. Retrying the delivery on
/// an ack failure would send the answer twice within one process for nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_delivery_survives_an_acknowledgement_that_cannot_be_recorded() {
    let token = CancellationToken::new();
    let poster = Gated::default();
    // A queue handle that opens no socket: the poster still answers, so the
    // delivery happens and only the acknowledgement can fail.
    let config =
        DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:1/".to_owned());
    let lanes = Lanes::new(
        Posters {
            slack: poster.clone(),
        },
        OutboundQueue::new(
            Dragonfly::unreachable(&config).expect("a well-formed URL builds a handle"),
        ),
        no_ledger::no_ledger(),
        token.clone(),
    );

    lanes
        .dispatch(Box::new(OutboundDelivery {
            id: EventId::of("1700000000002-0"),
            provider: PROVIDER.to_owned(),
            workspace_id: WORKSPACE.to_owned(),
            fleet_id: FLEET_ID.to_owned(),
            event_id: "unacknowledged-1".to_owned(),
            answer: "the run finished".to_owned(),
        }))
        .await;

    tokio::time::timeout(PATIENCE, async {
        while poster.delivered().is_empty() {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .expect("the destination takes the answer even though the queue is gone");

    assert_eq!(
        poster.delivered(),
        vec!["unacknowledged-1".to_owned()],
        "a failed acknowledgement must not make this process deliver twice"
    );

    token.cancel();
    lanes.drain().await;
}
