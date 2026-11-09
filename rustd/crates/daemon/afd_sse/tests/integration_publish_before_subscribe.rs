//! M244 Dimension 6.2: a frame published before a viewer's interest reaches
//! the publisher's node is covered by the backfill the tail asks for.
//!
//! ```text
//!   viewer subscribes on node 3 ── interest travels the routes ──► node 1
//!   publisher on node 1 publishes  ◄── in that window the frame finds no one
//! ```
//!
//! The tail cannot close that window: core NATS forwards only to interest a
//! node already knows. What it guarantees is the order of the opening, so the
//! page reloads the fleet's rows from Postgres before trusting the live feed:
//! `hello`, then `catching_up`, then frames. Each attempt opens a fresh tail on
//! node 3 and publishes on node 1 the instant the tail is returned, which is
//! the race; whether the frame arrives live or not, `catching_up` came first.
//! The UI's backfill on that signal is `fleet-stream-dispatch.ts`, tested there.

use std::time::Duration;

use af_core::env::{EnvSource as _, MapEnv, ProcessEnv};
use af_core::id::Uuid7;
use afd_bus::{Handle, Subject};
use afd_sse::{KIND_CATCHING_UP, Live, StreamLimit};
use futures_util::StreamExt as _;

type Fallible<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Attempts at the race; each opens one tail.
const ATTEMPTS: u32 = 50;
/// How long a published frame may take to arrive before it counts as missed.
const PATIENCE: Duration = Duration::from_millis(500);
/// The frame each attempt publishes.
const FRAME: &str = r#"{"kind":"event_admitted"}"#;

/// The lane's seeds, in node order.
fn seeds() -> Fallible<Vec<String>> {
    let raw = ProcessEnv
        .get("TEST_NATS_URL")
        .ok_or("TEST_NATS_URL is unset: run through make test-integration-rustd")?;
    Ok(raw.split(',').map(str::to_owned).collect())
}

/// A bus on one node only, sharing `namespace` with its peer.
async fn bus_on(seed: &str, namespace: &str) -> Fallible<Handle> {
    let env = MapEnv::from_pairs([
        (afd_bus::nats::URL_ENV, seed),
        (afd_bus::nats::NAMESPACE_ENV, namespace),
    ]);
    Ok(afd_bus::nats::connect(&afd_bus::nats::Config::from_env(&env)?).await?)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn frame_before_interest_is_covered_by_backfill() -> Fallible {
    let seeds = seeds()?;
    let namespace = format!("race_{}", std::process::id());
    let publisher = bus_on(seeds.first().ok_or("node 1")?, &namespace).await?;
    let viewer = bus_on(seeds.get(2).ok_or("node 3")?, &namespace).await?;
    let live = Live::new(viewer.clone(), StreamLimit::new(1));
    for attempt in 0..ATTEMPTS {
        // A fresh fleet per attempt, so no attempt inherits another's interest.
        let fleet = Uuid7::parse(&format!("0195b4ba-8d3a-7f13-8abc-{attempt:012x}"))?;
        let mut tail = live.tail_of(&fleet).await;
        publisher
            .publish(&Subject::FleetTail(fleet.clone()), FRAME)
            .await?;
        let hello = tail.next().await.ok_or("the tail opens")?;
        let catching_up = tail.next().await.ok_or("the tail announces the backfill")?;
        assert_eq!(hello.kind, "hello", "the stream opens with hello");
        assert_eq!(
            catching_up.kind, KIND_CATCHING_UP,
            "the backfill signal precedes any fleet frame"
        );
        // Live or lost to the window: either is correct once `catching_up`
        // came first, and a frame that does arrive is the published one.
        if let Ok(Some(frame)) = tokio::time::timeout(PATIENCE, tail.next()).await {
            assert_eq!(
                frame.kind, "event_admitted",
                "the published frame, unchanged"
            );
        }
    }
    publisher.shutdown().await?;
    viewer.shutdown().await?;
    Ok(())
}
