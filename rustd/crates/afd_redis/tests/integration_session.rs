//! Dimension 3.4 — the device-flow transition, under a real race.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_redis::session::{SessionState, SessionStatus, SessionStore, VerifyOutcome};

#[path = "support/redis_harness.rs"]
mod support;

use self::support::RedisHarness;

/// A code that is correct, and one that is not.
const GOOD_HMAC: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";
const BAD_HMAC: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
const FINGERPRINT: &str = "0000111122223333444455556666777788889999aaaabbbbccccddddeeeeffff";

fn approved_session(session_id: &str) -> SessionState {
    SessionState {
        session_id: session_id.to_owned(),
        status: SessionStatus::VerificationPending,
        cli_public_key: "cli-key".to_owned(),
        token_name: "laptop".to_owned(),
        dashboard_public_key: Some("dashboard-key".to_owned()),
        ciphertext: Some("encrypted-token".to_owned()),
        nonce: Some("nonce".to_owned()),
        verification_code_hmac_hex: Some(GOOD_HMAC.to_owned()),
        verification_attempts: 0,
        created_at_ms: 1_700_000_000_000,
        expires_at_ms: 1_700_000_300_000,
        approved_at_ms: Some(1_700_000_010_000),
        consumed_at_ms: None,
        aborted_reason: None,
        clerk_user_id: Some("user_1".to_owned()),
        consumed_client_fingerprint_hex: None,
        consume_payload_expires_at_ms: None,
    }
}

/// Dimension 3.4 — a hundred concurrent redemptions, exactly one success.
///
/// This is the whole reason the transition is a script. Written as `GET` then
/// `SET` from the client, every one of these tasks reads
/// `verification_pending`, every one writes `consumed`, and a device-flow code
/// is redeemed a hundred times. Redis runs a script body to completion, so the
/// window does not exist.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_session_transition_atomic() {
    let harness = RedisHarness::connect().await;
    let store = SessionStore::new(harness.redis.clone());
    let session_id = harness.name("session");

    store
        .put(&approved_session(&session_id))
        .await
        .expect("put");

    // A hundred, not two: two tasks can miss a race that a hundred find, and
    // the milestone's own concurrency bar is a hundred or more.
    let attempts = (0..100_u16).map(|index| {
        let store = store.clone();
        let session_id = session_id.clone();
        // Each caller looks like a different request, so a shared fingerprint
        // cannot be what serialises them.
        let fingerprint = format!("{FINGERPRINT}{index:04x}");
        tokio::spawn(async move {
            store
                .verify_and_consume(&session_id, GOOD_HMAC, 1_700_000_020_000, &fingerprint)
                .await
        })
    });

    let mut successes = 0_u32;
    let mut consumed = 0_u32;
    for attempt in attempts.collect::<Vec<_>>() {
        match attempt
            .await
            .expect("the task must not panic")
            .expect("verify")
        {
            VerifyOutcome::Success(payload) => {
                successes += 1;
                assert_eq!(payload.ciphertext, "encrypted-token");
                assert_eq!(payload.dashboard_public_key, "dashboard-key");
            }
            VerifyOutcome::Consumed | VerifyOutcome::Replay(_) => consumed += 1,
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    assert_eq!(
        successes, 1,
        "a device-flow code is redeemable exactly once"
    );
    assert_eq!(
        consumed, 99,
        "every loser must be told it was already consumed"
    );

    // The store agrees with what the callers were told.
    let after = store
        .get(&session_id)
        .await
        .expect("get")
        .expect("the session survives its redemption");
    assert_eq!(after.status, SessionStatus::Consumed);
    assert!(after.status.is_terminal());
    assert_eq!(after.consumed_at_ms, Some(1_700_000_020_000));

    cleanup(&harness, &session_id).await;
}

/// A wrong code counts against the session and, on the last attempt, ends it.
///
/// The lockout is the security property: without it a six-digit code is
/// brute-forceable inside the five-minute window.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_session_locks_out_after_repeated_wrong_codes() {
    let harness = RedisHarness::connect().await;
    let store = SessionStore::new(harness.redis.clone());
    let session_id = harness.name("session");
    store
        .put(&approved_session(&session_id))
        .await
        .expect("put");

    // Attempts one through four are counted and reported.
    for expected in 1..=4_u8 {
        let outcome = store
            .verify_and_consume(&session_id, BAD_HMAC, 1_700_000_020_000, FINGERPRINT)
            .await
            .expect("verify");
        assert_eq!(
            outcome,
            VerifyOutcome::InvalidCode(expected),
            "attempt {expected} must be counted, not swallowed"
        );
    }

    // The fifth trips the cap, and says so — a caller that saw `invalid_code`
    // here would tell the user to try again on a dead session.
    let outcome = store
        .verify_and_consume(&session_id, BAD_HMAC, 1_700_000_020_000, FINGERPRINT)
        .await
        .expect("verify");
    assert_eq!(outcome, VerifyOutcome::RateLimited);

    // And the correct code no longer works, because the session is aborted.
    let outcome = store
        .verify_and_consume(&session_id, GOOD_HMAC, 1_700_000_020_000, FINGERPRINT)
        .await
        .expect("verify");
    assert_eq!(
        outcome,
        VerifyOutcome::Aborted("rate_limit_exceeded".to_owned()),
        "a locked-out session must stay locked out"
    );

    let after = store.get(&session_id).await.expect("get").expect("blob");
    assert_eq!(after.status, SessionStatus::Aborted);
    assert_eq!(after.verification_attempts, 5);

    cleanup(&harness, &session_id).await;
}

/// A session that was never written, and one that is not yet approved.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_session_missing_and_unapproved_are_distinct() {
    let harness = RedisHarness::connect().await;
    let store = SessionStore::new(harness.redis.clone());

    let absent = harness.name("absent");
    assert_eq!(
        store
            .verify_and_consume(&absent, GOOD_HMAC, 1_700_000_020_000, FINGERPRINT)
            .await
            .expect("verify"),
        VerifyOutcome::Missing
    );
    assert!(store.get(&absent).await.expect("get").is_none());

    let pending_id = harness.name("pending");
    let mut pending = approved_session(&pending_id);
    pending.status = SessionStatus::Pending;
    pending.verification_code_hmac_hex = None;
    store.put(&pending).await.expect("put");

    assert_eq!(
        store
            .verify_and_consume(&pending_id, GOOD_HMAC, 1_700_000_020_000, FINGERPRINT)
            .await
            .expect("verify"),
        VerifyOutcome::NotApproved,
        "an unapproved session is not a wrong code — the user has nothing to fix"
    );

    cleanup(&harness, &pending_id).await;
}

/// The blob round-trips through Redis unchanged, which is what lets the Zig
/// daemon read what this writes.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_session_blob_round_trips() {
    let harness = RedisHarness::connect().await;
    let store = SessionStore::new(harness.redis.clone());
    let session_id = harness.name("session");
    let written = approved_session(&session_id);

    store.put(&written).await.expect("put");
    let read = store.get(&session_id).await.expect("get").expect("blob");
    assert_eq!(read, written, "the stored blob must survive the round trip");

    // The key carries a time-to-live, so an abandoned session cannot sit in
    // Redis forever holding a public key someone pasted.
    let key = afd_redis::session::session_key(&session_id);
    let mut cmd = redis::cmd("TTL");
    cmd.arg(&key);
    let ttl: i64 = harness.redis.command("TTL", &key, &cmd).await.expect("TTL");
    assert!(
        ttl > 0 && ttl <= 300,
        "a session must expire on its own, got {ttl}s"
    );

    cleanup(&harness, &session_id).await;
}

async fn cleanup(harness: &RedisHarness, session_id: &str) {
    let key = afd_redis::session::session_key(session_id);
    let mut cmd = redis::cmd("DEL");
    cmd.arg(&key);
    let _: Result<i64, _> = harness.redis.command("DEL", &key, &cmd).await;
}
