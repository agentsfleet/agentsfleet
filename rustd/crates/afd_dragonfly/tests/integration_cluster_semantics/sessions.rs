//! Dimension 6.2's session half: a race through a cold cache, and expiry.
//!
//! Split from the parent for length, and the cut is where the subject changes:
//! the parent asserts what a RETRY and a RELOAD do to the readiness index,
//! this asserts what they do to a device-flow code. The two share
//! [`super::forget_every_script`] because the hazard is the same one — both
//! one-time actions in this crate are Lua — and nothing else.

use afd_dragonfly::session::{
    SessionState, SessionStatus, SessionStore, VerifyOutcome, session_key,
};

use crate::support::RedisHarness;

use super::{ClusterHarness, forget_every_script};

/// A code that is correct. Nothing here presents a wrong one: the wrong-code
/// ladder is `test_session_locks_out_after_repeated_wrong_codes`, and what
/// this file adds is the cache state those tests do not vary.
const GOOD_HMAC: &str = "b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90a1";
const FINGERPRINT: &str = "1111222233334444555566667777888899990000aaaabbbbccccddddeeeeffff";

/// The payload a redeemed session hands back, as stored.
const STORED_CIPHERTEXT: &str = "encrypted-token";

/// Enough callers that a transition serialised per-caller instead of
/// per-server cannot pass by luck. The sibling atomicity proof uses the same
/// number for the same reason.
const CONCURRENT_REDEMPTIONS: u16 = 100;

/// The clock every session here is judged against, and the instants either
/// side of one lifetime.
const CREATED_AT_MS: i64 = 1_700_000_000_000;
const APPROVED_AT_MS: i64 = 1_700_000_010_000;
const EXPIRES_AT_MS: i64 = 1_700_000_300_000;
const WHILE_LIVE_MS: i64 = 1_700_000_020_000;

/// Bringing a key's expiry forward, so a five-minute lifetime can be proven in
/// a test instead of waited out.
const CMD_PEXPIRE: &str = "PEXPIRE";

/// The shortest expiry the server will take, and long enough after it that a
/// reap which is lazy rather than eager has still happened by the time the
/// redemption below asks.
const IMMINENT_MS: i64 = 1;
const AFTER_THE_REAP: std::time::Duration = std::time::Duration::from_millis(200);

/// A session the dashboard has approved and the CLI has not yet redeemed.
fn approved_session(session_id: &str) -> SessionState {
    SessionState {
        session_id: session_id.to_owned(),
        status: SessionStatus::VerificationPending,
        cli_public_key: "cli-key".to_owned(),
        token_name: "laptop".to_owned(),
        dashboard_public_key: Some("dashboard-key".to_owned()),
        ciphertext: Some(STORED_CIPHERTEXT.to_owned()),
        nonce: Some("nonce".to_owned()),
        verification_code_hmac_hex: Some(GOOD_HMAC.to_owned()),
        verification_attempts: 0,
        created_at_ms: CREATED_AT_MS,
        expires_at_ms: EXPIRES_AT_MS,
        approved_at_ms: Some(APPROVED_AT_MS),
        consumed_at_ms: None,
        aborted_reason: None,
        clerk_user_id: Some("user_1".to_owned()),
        consumed_client_fingerprint_hex: None,
        consume_payload_expires_at_ms: None,
    }
}

/// A hundred redemptions against a cache that was just emptied: one success.
///
/// `test_session_transition_atomic` races a WARM script. This races the reload
/// itself, which is the shape a node replacement produces in production: every
/// caller arrives at a server that has never seen the body. A driver that
/// reloaded per-caller without serialising would let two bodies run over one
/// key, and that is invisible when the script is already loaded.
pub(super) async fn a_race_through_a_cold_cache_still_redeems_once(
    harness: &RedisHarness,
    cluster: &ClusterHarness,
) {
    let store = SessionStore::new(harness.redis.clone());
    let session_id = harness.name("cold-race");
    store
        .put(&approved_session(&session_id))
        .await
        .expect("the approved session is stored");

    forget_every_script(cluster).await;

    let attempts = (0..CONCURRENT_REDEMPTIONS).map(|index| {
        let store = store.clone();
        let session_id = session_id.clone();
        // Distinct per caller, so a shared fingerprint cannot be what
        // serialises them and a replay cannot be counted as a success.
        let fingerprint = format!("{FINGERPRINT}{index:04x}");
        tokio::spawn(async move {
            store
                .verify_and_consume(&session_id, GOOD_HMAC, WHILE_LIVE_MS, &fingerprint)
                .await
        })
    });

    let mut successes = 0_u32;
    let mut refused = 0_u32;
    for attempt in attempts.collect::<Vec<_>>() {
        match attempt
            .await
            .expect("no caller panics")
            .expect("a cold cache is not a failure class")
        {
            VerifyOutcome::Success(payload) => {
                successes += 1;
                assert_eq!(
                    payload.ciphertext, STORED_CIPHERTEXT,
                    "the reloaded script hands back the payload it was stored with"
                );
            }
            VerifyOutcome::Consumed | VerifyOutcome::Replay(_) => refused += 1,
            other => panic!("a redemption answered {other:?} rather than winning or losing"),
        }
    }
    assert_eq!(
        successes, 1,
        "exactly one caller redeems the code, however cold the cache was"
    );
    assert_eq!(
        u32::from(CONCURRENT_REDEMPTIONS),
        successes + refused,
        "and every other caller is told so rather than faulting"
    );
}

/// A code outlives nothing: once the key expires, it cannot be redeemed.
///
/// # Why the assertion is `Missing` and not `Expired`
///
/// Worth stating, because the opposite reads as the obvious expectation and
/// is wrong. `verify_consume.lua` has an `{"expired"}` arm, but it fires on a
/// STORED `status == "expired"`, and no Rust path writes that status. The
/// script never compares `expires_at_ms` to `now_ms` either. Expiry here is
/// the KEY's, and `approve.lua:27-31` is what makes that sound: it re-stamps
/// `expires_at_ms` and resets the key's time-to-live in the same call, so the
/// field a client reads and the lifetime the server enforces cannot drift
/// apart. When the key goes, the session is `Missing`.
///
/// # Why that is worth a cluster test
///
/// Because the enforcement is entirely the engine's. This deployment's engine
/// changed, and an engine that kept a key past its time-to-live — or reaped it
/// only on access, on a node the redemption never reaches — would leave a
/// device-flow code redeemable after it was supposed to have died, with no
/// line of our own code at fault. That is the property asserted, against a
/// clock brought forward rather than waited out.
pub(super) async fn an_expired_code_is_gone_rather_than_redeemable(harness: &RedisHarness) {
    let store = SessionStore::new(harness.redis.clone());
    let expiring = harness.name("expiring");
    store
        .put(&approved_session(&expiring))
        .await
        .expect("the approved session is stored");

    // It IS redeemable right now — otherwise the assertion below would pass on
    // a session that was never valid in the first place.
    let key = session_key(&expiring);
    assert!(
        store
            .get(&expiring)
            .await
            .expect("the session reads")
            .is_some(),
        "the session is live before its key is expired"
    );

    let mut cmd = redis::cmd(CMD_PEXPIRE);
    cmd.arg(&key).arg(IMMINENT_MS);
    let rescheduled: i64 = harness
        .redis
        .command(CMD_PEXPIRE, &key, &cmd)
        .await
        .expect("the key takes a nearer expiry");
    assert_eq!(rescheduled, 1, "the key existed and its expiry was moved");
    tokio::time::sleep(AFTER_THE_REAP).await;

    let outcome = store
        .verify_and_consume(&expiring, GOOD_HMAC, WHILE_LIVE_MS, FINGERPRINT)
        .await
        .expect("an expired session is an outcome, not an error class");
    assert_eq!(
        outcome,
        VerifyOutcome::Missing,
        "the code cannot be redeemed once the key it lived in is gone — note the \
         clock passed in is still INSIDE the session's stated lifetime, so this \
         is the server's expiry doing the work and not an argument"
    );

    let never = harness.name("never-created");
    let outcome = store
        .verify_and_consume(&never, GOOD_HMAC, WHILE_LIVE_MS, FINGERPRINT)
        .await
        .expect("an unknown session is an outcome, not an error class");
    assert_eq!(
        outcome,
        VerifyOutcome::Missing,
        "and a session that never existed reads the same, which is the contract: \
         absence is one answer, not two"
    );
}
