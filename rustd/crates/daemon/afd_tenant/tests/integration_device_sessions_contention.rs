//! Device-login sessions under contention, as Postgres rows (M244 §5).
//!
//! The retired Lua scripts were atomic because Dragonfly ran each one alone.
//! Here the row lock is the authority, so each claim is proven with many
//! callers released together rather than one caller at a time.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::sync::Arc;

use af_core::error_code;
use af_core::id::Uuid7;
use afd_tenant::session::input::{Code, Opening};
use afd_tenant::session::{Fingerprint, SessionStore, Sessions};
use tokio::sync::Barrier;

use crate::device_flow_lane::{CODE, NOW, approval, origin, owner, surface};
use crate::session_harness::SessionHarness;

const CALLERS: usize = 100;

/// Opens and approves one session for `approver`.
async fn approved(sessions: &Sessions, approver: &str) -> String {
    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), NOW)
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), approver)
        .await
        .expect("approve");
    opened.session_id
}

/// The row's status and attempt count, read straight from the table.
async fn row(harness: &SessionHarness, session_id: &str) -> (String, i16) {
    let id = Uuid7::parse(session_id).expect("a session id");
    let mut connection = harness.database.acquire().await.expect("a connection");
    sqlx::query_as(
        "SELECT status, verification_attempts FROM identity.device_sessions WHERE id = $1",
    )
    .bind(&id)
    .fetch_one(&mut *connection)
    .await
    .expect("the session row")
}

/// Moves a session's whole life into the past.
const BACKDATE_EXPIRY: &str =
    "UPDATE identity.device_sessions SET expires_at = now() - interval '1 second' WHERE id = $1";
/// Closes a consumed session's replay window.
const BACKDATE_REPLAY: &str = "UPDATE identity.device_sessions SET replay_expires_at = now() - interval '1 second' WHERE id = $1";

/// Rewrites one deadline, as a clock that moved on would.
async fn backdate(harness: &SessionHarness, session_id: &str, statement: &'static str) {
    let id = Uuid7::parse(session_id).expect("a session id");
    let mut connection = harness.database.acquire().await.expect("a connection");
    sqlx::query(statement)
        .bind(&id)
        .execute(&mut *connection)
        .await
        .expect("the deadline moves");
}

/// Releases `CALLERS` verifies of one session together.
async fn verify_together(
    sessions: &Sessions,
    session_id: &str,
    code: &'static str,
    fingerprint: &Fingerprint,
) -> Vec<afd_tenant::Result<afd_tenant::session::Redeemed>> {
    let barrier = Arc::new(Barrier::new(CALLERS));
    let mut callers = Vec::new();
    for _ in 0..CALLERS {
        let (sessions, barrier) = (sessions.clone(), Arc::clone(&barrier));
        let (session_id, fingerprint) = (session_id.to_owned(), fingerprint.clone());
        callers.push(tokio::spawn(async move {
            barrier.wait().await;
            let code = Code::parse(code).expect("digits");
            sessions.verify(&session_id, &code, &fingerprint).await
        }));
    }
    let mut answers = Vec::new();
    for caller in callers {
        answers.push(caller.await.expect("a caller ran"));
    }
    answers
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn concurrent_wrong_codes_lock_out_at_five() {
    let harness = SessionHarness::connect().await;
    let sessions = surface(&harness);
    let session_id = approved(&sessions, &owner("lockout")).await;

    let answers = verify_together(&sessions, &session_id, "000000", &origin(&session_id)).await;
    let rate_limited = afd_tenant::error::detail::DETAIL_SESSION_RATE_LIMITED;
    let aborted = afd_tenant::error::detail::DETAIL_SESSION_ABORTED;
    let (mut invalid, mut locked, mut dead) = (0, 0, 0);
    for answer in answers {
        let error = answer.expect_err("a wrong code never redeems");
        if error.code() == error_code::VERIFICATION_FAILED {
            invalid += 1;
        } else if error.detail() == rate_limited {
            locked += 1;
        } else if error.detail() == aborted {
            dead += 1;
        }
    }

    assert_eq!((invalid, locked, dead), (4, 1, 95));
    assert_eq!(row(&harness, &session_id).await, ("aborted".to_owned(), 5));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn concurrent_redemptions_consume_once() {
    let harness = SessionHarness::connect().await;
    let sessions = surface(&harness);
    let session_id = approved(&sessions, &owner("redeem")).await;

    let answers = verify_together(&sessions, &session_id, CODE, &origin(&session_id)).await;
    let redeemed: Vec<_> = answers
        .into_iter()
        .map(|answer| answer.expect("the right code redeems or replays"))
        .collect();

    assert_eq!(
        redeemed.iter().filter(|r| !r.repeated).count(),
        1,
        "one Success"
    );
    assert_eq!(
        redeemed.iter().filter(|r| r.repeated).count(),
        99,
        "99 Replay"
    );
    assert_eq!(row(&harness, &session_id).await.0, "consumed");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn abort_and_consume_race_end_in_one_state() {
    let harness = SessionHarness::connect().await;
    let sessions = surface(&harness);
    let approver = owner("abort_race");
    let session_id = approved(&sessions, &approver).await;

    let barrier = Arc::new(Barrier::new(CALLERS));
    let mut racers = Vec::new();
    for index in 0..CALLERS {
        let (sessions, barrier) = (sessions.clone(), Arc::clone(&barrier));
        let (session_id, approver) = (session_id.clone(), approver.clone());
        racers.push(tokio::spawn(async move {
            barrier.wait().await;
            if index % 2 == 0 {
                sessions
                    .cancel_all(&approver)
                    .await
                    .expect("the store answers");
                false
            } else {
                let code = Code::parse(CODE).expect("digits");
                let fingerprint = origin(&session_id);
                sessions
                    .verify(&session_id, &code, &fingerprint)
                    .await
                    .is_ok_and(|redeemed| !redeemed.repeated)
            }
        }));
    }
    let mut successes = 0;
    for racer in racers {
        successes += usize::from(racer.await.expect("a racer ran"));
    }

    let (status, _) = row(&harness, &session_id).await;
    let expected = match status.as_str() {
        "consumed" => 1,
        "aborted" => 0,
        _ => usize::MAX,
    };
    assert_eq!(
        successes, expected,
        "the race left `{status}` after {successes} redemptions"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn replay_window_answers_only_the_same_request() {
    let harness = SessionHarness::connect().await;
    let sessions = surface(&harness);
    let session_id = approved(&sessions, &owner("replay")).await;
    let code = Code::parse(CODE).expect("digits");
    let caller = origin(&session_id);
    let first = sessions
        .verify(&session_id, &code, &caller)
        .await
        .expect("redeems");
    assert!(!first.repeated);

    let again = sessions
        .verify(&session_id, &code, &caller)
        .await
        .expect("the same request replays inside 60 s");
    assert!(again.repeated);
    let stranger = Fingerprint::of("198.51.100.9", "curl/8", &session_id);
    let refused = sessions
        .verify(&session_id, &code, &stranger)
        .await
        .expect_err("another fingerprint is consumed");
    assert_eq!(refused.code(), error_code::SESSION_CONSUMED);

    backdate(&harness, &session_id, BACKDATE_REPLAY).await;
    let late = sessions
        .verify(&session_id, &code, &caller)
        .await
        .expect_err("a closed window is consumed");
    assert_eq!(late.code(), error_code::SESSION_CONSUMED);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn expired_session_is_refused_and_swept() {
    let harness = SessionHarness::connect().await;
    let sessions = surface(&harness);
    let session_id = approved(&sessions, &owner("expiry")).await;
    backdate(&harness, &session_id, BACKDATE_EXPIRY).await;

    let refused = sessions
        .verify(
            &session_id,
            &Code::parse(CODE).expect("digits"),
            &origin(&session_id),
        )
        .await
        .expect_err("an expired session refuses");
    assert_eq!(refused.code(), error_code::SESSION_EXPIRED);

    let store = SessionStore::new(harness.database.clone());
    let swept = store.sweep_expired(1_000).await.expect("the sweep runs");
    assert!(swept >= 1, "a pass removes the expired row");
    assert!(!row_exists(&harness, &session_id).await, "the row is gone");
}

async fn row_exists(harness: &SessionHarness, session_id: &str) -> bool {
    let id = Uuid7::parse(session_id).expect("a session id");
    let mut connection = harness.database.acquire().await.expect("a connection");
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM identity.device_sessions WHERE id = $1)")
        .bind(&id)
        .fetch_one(&mut *connection)
        .await
        .expect("the existence check reads")
}
