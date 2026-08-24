//! The readiness index, and the connection's own surface.
//!
//! Split from `integration_streams.rs` per RULE FLL, along the seam that
//! matters: that file is the event stream, this one is the index a lease poll
//! reads before it opens a Postgres connection, plus the client both ride on.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_redis::ready::ReadyIndex;

#[path = "support/redis_harness.rs"]
mod support;

use self::support::RedisHarness;

/// The readiness index only clears a mark the caller actually saw.
///
/// The race this closes: a poll finds a fleet idle and moves to clear it while
/// ingress appends and re-marks. An unconditional delete erases a mark for
/// genuinely undelivered work, and nothing rediscovers it until a sweep.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_ready_index_clear_respects_the_token() {
    let harness = RedisHarness::connect().await;
    let index = ReadyIndex::new(harness.redis.clone());
    let fleet = harness.name("fleet");

    let observed = index.mark(&fleet, "token-a").await.expect("mark");
    assert!(
        index
            .peek(50)
            .await
            .expect("peek")
            .iter()
            .any(|ready| ready.fleet_id == fleet),
        "a marked fleet must be visible to a poll"
    );

    // Ingress marks again — a new generation — while the poll still holds the
    // token it read.
    index.mark(&fleet, "token-b").await.expect("re-mark");

    assert!(
        !index
            .clear_if_unchanged(&fleet, &observed)
            .await
            .expect("clear"),
        "a stale token must not clear a fleet that was re-marked"
    );
    assert!(
        index
            .peek(50)
            .await
            .expect("peek")
            .iter()
            .any(|ready| ready.fleet_id == fleet),
        "the newer mark must survive the stale clear"
    );

    // The current token does clear it.
    let current = index.mark(&fleet, "token-c").await.expect("mark");
    assert!(
        index
            .clear_if_unchanged(&fleet, &current)
            .await
            .expect("clear"),
        "the token the caller observed must clear the fleet"
    );

    cleanup_fields(&harness, &fleet).await;
}

/// The index's read surface: a count, an emptiness question, and a sample.
///
/// `peek` is what a lease poll calls before it opens a Postgres connection, so
/// a pairing bug here — a field read as a token, or a truncated last pair —
/// sends every replica at the wrong fleet.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_ready_index_read_surface() {
    let harness = RedisHarness::connect().await;
    let index = ReadyIndex::new(harness.redis.clone());

    let before = index.len().await.expect("len");
    let fleets: Vec<String> = (0..3).map(|n| harness.name(&format!("fleet{n}"))).collect();
    for (position, fleet) in fleets.iter().enumerate() {
        index
            .mark(fleet, &format!("token-{position}"))
            .await
            .expect("mark");
    }

    assert_eq!(index.len().await.expect("len"), before + 3);
    assert!(!index.is_empty().await.expect("is_empty"));

    // Every sampled pair must be a field with ITS value, not a shifted pairing.
    let sample = index.peek(100).await.expect("peek");
    for fleet in &fleets {
        let found = sample
            .iter()
            .find(|ready| &ready.fleet_id == fleet)
            .expect("a marked fleet must be sampled");
        let position = fleets
            .iter()
            .position(|candidate| candidate == fleet)
            .expect("known");
        assert_eq!(
            found.token.as_str(),
            format!("token-{position}"),
            "the sample paired a fleet with another fleet's token"
        );
    }

    for fleet in &fleets {
        cleanup_fields(&harness, fleet).await;
    }
}

/// The connection answers for itself, and a certificate path that is not there
/// is a config failure rather than an outage.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_client_reports_its_own_configuration() {
    let harness = RedisHarness::connect().await;
    assert_eq!(harness.redis.role(), afd_redis::RedisRole::Default);
    assert_eq!(
        harness.redis.request_timeout(),
        std::time::Duration::from_secs(5)
    );
    harness
        .redis
        .ping()
        .await
        .expect("a live Redis answers PING");

    let missing_ca = RedisHarness::config().with_ca_cert_file(Some("/nonexistent/ca.crt".into()));
    let error = afd_redis::Redis::connect(&missing_ca)
        .await
        .expect_err("a certificate authority that is not there must refuse");
    assert!(
        error.is_config(),
        "an unreadable certificate is a misconfiguration, not an outage: {error}"
    );
    assert!(!error.is_unavailable(), "got {error}");
}

/// A command that outlives its deadline is a timeout, named, not a hang.
///
/// Invariant 4 of the milestone is that every I/O deadline is a
/// `tokio::time::timeout` at the call site. A deadline nothing ever trips is
/// indistinguishable from no deadline at all, so this trips one deterministically:
/// `BLPOP` on a key nothing ever pushes to blocks the server for seconds, and
/// the client's budget is a fraction of that. Racing a fast command against a
/// tiny budget would not do — it completes inside the timer's first tick, which
/// is what the first version of this test discovered.
///
/// It also shows why a multiplexed connection must never carry a blocking
/// command in production: this one holds the socket for its whole duration,
/// which is why [`crate::streams`] reads never pass `BLOCK` and pub/sub gets a
/// connection of its own.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_a_command_past_its_deadline_is_a_timeout() {
    let harness = RedisHarness::connect().await;
    let impatient_config = RedisHarness::config().with_request_timeout(Duration::from_millis(50));
    let impatient = afd_redis::Redis::connect(&impatient_config)
        .await
        .expect("connecting is not the part under test");

    let key = harness.name("never_pushed");
    let mut blocking = redis::cmd("BLPOP");
    blocking.arg(&key).arg(5);

    let started = std::time::Instant::now();
    let error = impatient
        .command::<Option<Vec<String>>>("BLPOP", &key, &blocking)
        .await
        .expect_err("a 50ms budget cannot outlast a 5s block");

    assert!(
        error.is_unavailable(),
        "a deadline that passed is the datastore not answering in time: {error}"
    );
    assert!(!error.is_command(), "nothing was refused; nothing answered");
    assert_eq!(error.code().as_str(), "UZ-STARTUP-004");
    assert!(
        error.to_string().contains("BLPOP"),
        "the failure must name the command that hung: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the deadline must cut the wait short, not ride it out: {:?}",
        started.elapsed()
    );
}

/// Removes a fleet's field from the shared index, so one test's marks never
/// appear in another's sample.
async fn cleanup_fields(harness: &RedisHarness, fleet: &str) {
    let mut cmd = redis::cmd("HDEL");
    cmd.arg(afd_redis::ready::READY_INDEX_KEY).arg(fleet);
    let _: Result<i64, _> = harness
        .redis
        .command("HDEL", afd_redis::ready::READY_INDEX_KEY, &cmd)
        .await;
}

/// Every way opening a connection can fail, told apart.
///
/// Three different causes that a caller might otherwise see as one "could not
/// connect": a URL this client accepts but the driver does not, a certificate
/// file that exists and is not a certificate, and a port with nothing behind
/// it. The first two are misconfiguration an operator fixes in seconds once
/// the message says which; the third is an outage.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_connection_failures_name_their_cause() {
    use afd_redis::config::{RedisConfig, RedisRole};

    // Our scheme check passes; the driver's own parser refuses the rest.
    let malformed = RedisConfig::from_url(RedisRole::Default, "redis://%%%invalid%%%".to_owned());
    let error = afd_redis::Redis::connect(&malformed)
        .await
        .expect_err("a URL the driver cannot parse must refuse");
    assert!(!error.is_command(), "nothing was ever sent: {error}");

    // A certificate authority file that is not a certificate.
    let junk = std::env::temp_dir().join(format!("afd-not-a-cert-{}.pem", std::process::id()));
    std::fs::write(&junk, b"this is not a certificate\n").expect("write the junk file");
    let bad_pem = RedisHarness::config().with_ca_cert_file(Some(junk.clone()));
    let error = afd_redis::Redis::connect(&bad_pem)
        .await
        .expect_err("a file that is not a certificate must refuse");
    assert!(!error.is_command(), "got {error}");
    let _ = std::fs::remove_file(&junk);

    // Nothing listening.
    let dead = RedisConfig::from_url(RedisRole::Default, "redis://127.0.0.1:1".to_owned())
        .with_request_timeout(Duration::from_millis(500));
    let error = afd_redis::Redis::connect(&dead)
        .await
        .expect_err("nothing is listening on port 1");
    assert!(
        error.is_unavailable(),
        "an unreachable Redis is an outage: {error}"
    );
}
