//! The device-flow login handshake, against live Postgres.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
//!
//! Everything here goes through [`Sessions`] rather than through the store
//! beneath it, deliberately: the store's own atomicity is proven under
//! contention in `integration_device_sessions_contention`, and what this file
//! adds is the SURFACE — which refusal each outcome becomes, and which of them
//! a client can tell apart.
//!
//! Split by concern: this file holds the handshake and its refusals,
//! `integration_device_flow_lifecycle` the terminal states and cancels, and
//! `integration_device_flow_races` the concurrent callers and the attempt
//! ceiling. The fixture they share is `support/device_flow_lane.rs`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::error_code;
use afd_tenant::session::SessionStatus;
use afd_tenant::session::input::{Code, Opening};

use crate::device_flow_lane::{APPROVER, CODE, NOW, approval, origin, surface};
use crate::session_harness::SessionHarness;

/// Create, approve, verify, and the envelope comes back whole.
///
/// The daemon's whole job in this flow is to relay: what goes in at `/approve`
/// is what comes out at `/verify`, byte for byte, and nothing in between opens
/// it. The assertion is that equality — not that a credential was minted, which
/// happens in the client after it decrypts what this hands back.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_device_flow_happy_path() {
    let harness = SessionHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(
            &Opening::parse("cli-key", "Indy's laptop").expect("bounds"),
            NOW,
        )
        .await
        .expect("open");
    assert_eq!(
        opened.login_url,
        format!(
            "https://app-dev.agentsfleet.net/cli-auth/{}",
            opened.session_id
        ),
        "the configured dashboard's trailing slash must not double up"
    );

    let waiting = sessions.poll(&opened.session_id).await.expect("poll");
    assert_eq!(waiting.status, SessionStatus::Pending);
    assert_eq!(waiting.cli_public_key, "cli-key");
    assert_eq!(waiting.token_name, "Indy's laptop");

    sessions
        .approve(&opened.session_id, &approval(), APPROVER)
        .await
        .expect("approve");

    let waiting = sessions
        .poll(&opened.session_id)
        .await
        .expect("poll after approve");
    assert_eq!(waiting.status, SessionStatus::VerificationPending);

    let redeemed = sessions
        .verify(
            &opened.session_id,
            &Code::parse(CODE).expect("digits"),
            &origin(&opened.session_id),
        )
        .await
        .expect("verify");
    assert_eq!(redeemed.dashboard_public_key, "dash-key");
    assert_eq!(redeemed.ciphertext, "sealed-credential");
    assert_eq!(redeemed.nonce, "nonce-12");
    assert!(!redeemed.repeated, "the first redemption is not a replay");
}

/// The session row holds a DIGEST of the code and never the code.
///
/// Split from the handshake above because it is a different claim about a
/// different thing: that one is about what a client receives, this is about
/// what an operator with a database console can read. If the six digits were
/// stored, anybody who can read the table could finish somebody else's login.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_device_flow_stores_only_the_code_digest() {
    let harness = SessionHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), NOW)
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), APPROVER)
        .await
        .expect("approve");

    let id = af_core::id::Uuid7::parse(&opened.session_id).expect("a session id");
    let mut connection = harness.database.acquire().await.expect("a connection");
    let (digest, row): (Option<Vec<u8>>, String) = sqlx::query_as(
        "SELECT verification_code_hmac, row_to_json(s)::text \
         FROM identity.device_sessions s WHERE id = $1",
    )
    .bind(&id)
    .fetch_one(&mut *connection)
    .await
    .expect("the session was written");
    let digest = digest.expect("an approved session carries a digest");
    assert_eq!(digest.len(), 32, "a SHA-256 tag");
    assert!(
        !row.contains(CODE),
        "no column of the stored session may carry the six digits"
    );
}

/// Every malformed field earns its own documented code.
///
/// A unit-level claim about [`Opening`] and [`Approval`] already exists; what
/// this adds is that the refusal survives the whole way out of the SERVICE,
/// against live Postgres, rather than being swallowed by a store error on the
/// way.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_device_flow_rejects_malformed() {
    let harness = SessionHarness::connect().await;
    let sessions = surface(&harness);

    // An id that is not an identifier reads as "not found", never as
    // "malformed": telling the two apart would make the poll an oracle for
    // which ids exist.
    let refusal = sessions.poll("not-a-uuid").await.expect_err("refused");
    assert_eq!(refusal.code(), error_code::SESSION_NOT_FOUND);

    // A well-formed id for a session nobody opened reads the same way.
    let stranger = "01924f4e-0000-7000-8000-0000000000ff";
    let refusal = sessions.poll(stranger).await.expect_err("refused");
    assert_eq!(refusal.code(), error_code::SESSION_NOT_FOUND);

    // Approving a session that is not there is not-found, not a conflict.
    let refusal = sessions
        .approve(stranger, &approval(), APPROVER)
        .await
        .expect_err("refused");
    assert_eq!(refusal.code(), error_code::SESSION_NOT_FOUND);

    // A code presented before any human approved is a CONFLICT rather than a
    // rejection: the session is still approvable, so the caller waits.
    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), NOW)
        .await
        .expect("open");
    let refusal = sessions
        .verify(
            &opened.session_id,
            &Code::parse(CODE).expect("digits"),
            &origin(&opened.session_id),
        )
        .await
        .expect_err("refused");
    assert_eq!(refusal.code(), error_code::SESSION_NOT_APPROVED);

    // A wrong code against an approved session is retryable, and says so with a
    // different code from every terminal state above.
    sessions
        .approve(&opened.session_id, &approval(), APPROVER)
        .await
        .expect("approve");
    let refusal = sessions
        .verify(
            &opened.session_id,
            &Code::parse("000000").expect("digits"),
            &origin(&opened.session_id),
        )
        .await
        .expect_err("refused");
    assert_eq!(refusal.code(), error_code::VERIFICATION_FAILED);
}
