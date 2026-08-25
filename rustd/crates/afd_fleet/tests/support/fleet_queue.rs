//! The lane's Redis, and the two things a lease test has to put in it.
//!
//! Separate from `fleet_fixtures.rs` because the two harnesses have opposite
//! lifetimes. That file creates a DATABASE PER TEST and drops it, which is what
//! keeps row assertions independent. Redis has no such equivalent: the
//! readiness index is one hash at a fixed key and the streams are keyed by
//! fleet, so isolation here comes from every test declaring its own fleet ids
//! rather than from tearing anything down.
//!
//! That difference is why a leaked readiness mark is harmless: the candidate
//! query joins `core.fleets` in the test's OWN database, so another test's
//! fleet id cannot survive the filter even when its mark is still in the index.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_redis::{FleetStreams, ReadyIndex, Redis, RedisConfig, RedisRole};

/// The lane's Redis URL.
const URL_KNOB: &str = "TEST_REDIS_URL";

/// The lane's Redis certificate authority, when it serves TLS.
const CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// The configuration the lane hands these suites.
pub(crate) fn config() -> RedisConfig {
    let url = std::env::var(URL_KNOB).unwrap_or_else(|_error| {
        panic!("{URL_KNOB} is unset — run these through `make test-integration-rustd`")
    });
    RedisConfig::from_url(RedisRole::Default, url)
        .with_ca_cert_file(std::env::var(CA_KNOB).ok().map(Into::into))
}

/// Connects to the lane's Redis.
pub(crate) async fn connect() -> Redis {
    Redis::connect(&config())
        .await
        .expect("the lane's Redis must be reachable")
}

/// Puts one event on a fleet's stream and marks the fleet ready.
///
/// Both halves, because either alone is a state the daemon never produces:
/// ingress appends and marks in one path, and a mark with no entry would make
/// the assignment pass look broken when it is the fixture that is.
///
/// The field names are `event_envelope.zig`'s `encodeForXAdd` argv — the
/// producer's side of the contract `assign.rs` reads.
pub(crate) async fn enqueue(
    queue: &Redis,
    fleet: &str,
    workspace: &str,
    actor: &str,
    event_type: &str,
    request_json: &str,
    created_at: i64,
) -> String {
    let streams = FleetStreams::new(queue.clone());
    streams
        .ensure_group(fleet)
        .await
        .expect("the consumer group must exist before a read");
    let created = created_at.to_string();
    let id = streams
        .append(
            fleet,
            &[
                ("type", event_type),
                ("actor", actor),
                ("workspace_id", workspace),
                ("request", request_json),
                ("created_at", &created),
            ],
        )
        .await
        .expect("the append must land");
    mark_ready(queue, fleet).await;
    id.as_str().to_owned()
}

/// Marks a fleet ready so the readiness peek can surface it.
pub(crate) async fn mark_ready(queue: &Redis, fleet: &str) {
    ReadyIndex::new(queue.clone())
        .mark(fleet, fleet)
        .await
        .expect("the readiness mark must land");
}

/// Removes a fleet's readiness mark, so one test's fleet does not crowd the
/// bounded peek another test depends on.
pub(crate) async fn clear_ready(queue: &Redis, fleet: &str) {
    let index = ReadyIndex::new(queue.clone());
    let token = index
        .mark(fleet, fleet)
        .await
        .expect("re-marking to obtain the token must succeed");
    let _cleared = index.clear_if_unchanged(fleet, &token).await;
}
