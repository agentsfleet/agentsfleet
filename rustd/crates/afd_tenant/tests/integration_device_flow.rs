//! §1 — the device-flow login handshake, against a real queue.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
//!
//! Everything here goes through [`Sessions`] rather than through the store
//! beneath it, deliberately: the store's own atomicity is M176's proof, and
//! what this milestone adds is the SURFACE — which refusal each outcome
//! becomes, and which of them a client can tell apart.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::SecretBytes;
use afd_redis::SessionStore;
use afd_tenant::session::input::{Approval, Code, Opening};
use afd_tenant::session::{Cancelled, Fingerprint, SessionStatus, Sessions};

#[path = "support/redis_harness.rs"]
mod support;

use self::support::RedisHarness;

/// The dashboard this suite's login URLs are built against.
const APP_URL: &str = "https://app-dev.agentsfleet.net/";

/// The digits a person reads out of the browser in these tests.
const CODE: &str = "428193";

/// The identity that approves them.
const APPROVER: &str = "user_2abcDEF";

/// A fixed instant, so nothing here depends on when it runs.
const NOW_MS: i64 = 1_700_000_000_000;

/// The login surface, over the lane's queue.
fn surface(harness: &RedisHarness) -> Sessions {
    Sessions::new(
        SessionStore::new(harness.redis.clone()),
        SecretBytes::new(b"c0ffee".repeat(8)),
        Entropy::new(),
        APP_URL,
    )
}

/// The instant every write in one of these tests is stamped with.
const fn now() -> UnixMillis {
    UnixMillis::from_millis(NOW_MS)
}

/// An approval carrying this suite's fixed envelope.
fn approval() -> Approval<'static> {
    Approval::parse("dash-key", "sealed-credential", "nonce-12", CODE).expect("bounds")
}

/// A caller's identity for the replay window.
fn origin(session_id: &str) -> Fingerprint {
    Fingerprint::of("203.0.113.7", "agentsfleet/1.0", session_id)
}

/// Dimension 1.1 — create, approve, verify, and the envelope comes back whole.
///
/// The daemon's whole job in this flow is to relay: what goes in at `/approve`
/// is what comes out at `/verify`, byte for byte, and nothing in between opens
/// it. The assertion is that equality — not that a credential was minted, which
/// happens in the client after it decrypts what this hands back.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_device_flow_happy_path() {
    let harness = RedisHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(
            &Opening::parse("cli-key", "Indy's laptop").expect("bounds"),
            now(),
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
        .approve(&opened.session_id, &approval(), APPROVER, now())
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
            now(),
        )
        .await
        .expect("verify");
    assert_eq!(redeemed.dashboard_public_key, "dash-key");
    assert_eq!(redeemed.ciphertext, "sealed-credential");
    assert_eq!(redeemed.nonce, "nonce-12");
    assert!(!redeemed.repeated, "the first redemption is not a replay");
}

/// Dimension 1.1 — the queue holds a DIGEST of the code and never the code.
///
/// Split from the handshake above because it is a different claim about a
/// different thing: that one is about what a client receives, this is about
/// what an operator with a Redis console can read. If the six digits were
/// stored, anybody who can read the queue could finish somebody else's login.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_device_flow_stores_only_the_code_digest() {
    let harness = RedisHarness::connect().await;
    let sessions = surface(&harness);
    let store = SessionStore::new(harness.redis.clone());

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("approve");

    let stored = store
        .get(&opened.session_id)
        .await
        .expect("read")
        .expect("the session was written");
    let digest = stored
        .verification_code_hmac_hex
        .as_deref()
        .expect("an approved session carries a digest");
    assert_eq!(digest.len(), 64, "SHA-256 rendered as lower-case hex");
    assert!(
        !digest.contains(CODE),
        "the plaintext code must not survive anywhere in the blob"
    );
    let blob = serde_json::to_string(&stored).expect("re-encode");
    assert!(
        !blob.contains(CODE),
        "no field of the stored session may carry the six digits"
    );
}

/// Dimension 1.2 — every malformed field earns its own documented code.
///
/// A unit-level claim about [`Opening`] and [`Approval`] already exists; what
/// this adds is that the refusal survives the whole way out of the SERVICE,
/// against a live queue, rather than being swallowed by a store error on the
/// way.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_device_flow_rejects_malformed() {
    let harness = RedisHarness::connect().await;
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
        .approve(stranger, &approval(), APPROVER, now())
        .await
        .expect_err("refused");
    assert_eq!(refusal.code(), error_code::SESSION_NOT_FOUND);

    // A code presented before any human approved is a CONFLICT rather than a
    // rejection: the session is still approvable, so the caller waits.
    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    let refusal = sessions
        .verify(
            &opened.session_id,
            &Code::parse(CODE).expect("digits"),
            &origin(&opened.session_id),
            now(),
        )
        .await
        .expect_err("refused");
    assert_eq!(refusal.code(), error_code::SESSION_NOT_APPROVED);

    // A wrong code against an approved session is retryable, and says so with a
    // different code from every terminal state above.
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("approve");
    let refusal = sessions
        .verify(
            &opened.session_id,
            &Code::parse("000000").expect("digits"),
            &origin(&opened.session_id),
            now(),
        )
        .await
        .expect_err("refused");
    assert_eq!(refusal.code(), error_code::VERIFICATION_FAILED);
}

/// Dimension 1.2 — a redeemed session is terminal, and reads as gone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_device_flow_terminal_states_are_terminal() {
    let harness = RedisHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("approve");
    sessions
        .verify(
            &opened.session_id,
            &Code::parse(CODE).expect("digits"),
            &origin(&opened.session_id),
            now(),
        )
        .await
        .expect("verify");

    // A poll after redemption is 410-shaped, not 200-with-a-status: the client
    // is told the session is over rather than being handed a state to render.
    let refusal = sessions
        .poll(&opened.session_id)
        .await
        .expect_err("consumed");
    assert_eq!(refusal.code(), error_code::SESSION_CONSUMED);

    // And a cancel of it cannot un-redeem it.
    let refusal = sessions
        .cancel(&opened.session_id, APPROVER)
        .await
        .expect_err("consumed");
    assert_eq!(refusal.code(), error_code::SESSION_CONSUMED);
}

/// Dimension 1.2 — a login is cancellable by its owner and by nobody else.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_device_flow_cancel_is_owner_checked_and_idempotent() {
    let harness = RedisHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("approve");

    let refusal = sessions
        .cancel(&opened.session_id, "user_someone_else")
        .await
        .expect_err("foreign");
    assert_eq!(refusal.code(), error_code::AUTH_FORBIDDEN);

    assert_eq!(
        sessions
            .cancel(&opened.session_id, APPROVER)
            .await
            .expect("cancel"),
        Cancelled::Now,
        "the first cancel performs the transition"
    );
    assert_eq!(
        sessions
            .cancel(&opened.session_id, APPROVER)
            .await
            .expect("cancel"),
        Cancelled::Already,
        "a repeat changes nothing, so nothing is audited twice"
    );

    let refusal = sessions
        .poll(&opened.session_id)
        .await
        .expect_err("aborted");
    assert_eq!(refusal.code(), error_code::SESSION_ABORTED);
}

/// Dimension 1.3 — two dashboards click Approve, and exactly one wins.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_device_flow_state_races_on_approve() {
    let harness = RedisHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");

    // A hundred, not two: two callers can miss a window a hundred find, and the
    // repository's contention bar is a hundred or more.
    let attempts = (0..100_u16).map(|_| {
        let sessions = sessions.clone();
        let session_id = opened.session_id.clone();
        tokio::spawn(async move {
            sessions
                .approve(&session_id, &approval(), APPROVER, now())
                .await
        })
    });

    let mut approved = 0_u32;
    let mut conflicted = 0_u32;
    for attempt in futures_util::future::join_all(attempts).await {
        match attempt.expect("task") {
            Ok(()) => approved += 1,
            Err(refusal) => {
                assert_eq!(refusal.code(), error_code::SESSION_ALREADY_APPROVED);
                conflicted += 1;
            }
        }
    }
    assert_eq!(approved, 1, "one approval is recorded, whatever the racing");
    assert_eq!(conflicted, 99);
}

/// Dimension 1.3 — a hundred callers present the right code, and one redeems.
///
/// The others are refused as ALREADY CONSUMED rather than replayed, because
/// each is given a distinct fingerprint: the replay window is for the caller
/// who asked first losing its reply, not for a crowd sharing one answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_device_flow_state_races_on_verify() {
    let harness = RedisHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("approve");

    let attempts = (0..100_u16).map(|index| {
        let sessions = sessions.clone();
        let session_id = opened.session_id.clone();
        let agent = format!("agentsfleet/1.0 ({index})");
        tokio::spawn(async move {
            sessions
                .verify(
                    &session_id,
                    &Code::parse(CODE).expect("digits"),
                    &Fingerprint::of("203.0.113.7", &agent, &session_id),
                    now(),
                )
                .await
        })
    });

    let mut redeemed = 0_u32;
    for attempt in futures_util::future::join_all(attempts).await {
        match attempt.expect("task") {
            Ok(payload) => {
                assert!(!payload.repeated, "a distinct caller is not a replay");
                redeemed += 1;
            }
            Err(refusal) => assert_eq!(refusal.code(), error_code::SESSION_CONSUMED),
        }
    }
    assert_eq!(redeemed, 1, "a device-flow code is redeemed exactly once");
}

/// Dimension 1.3 — the caller who asked first may ask again, and only them.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_device_flow_replays_for_the_original_caller_only() {
    let harness = RedisHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("approve");

    let code = Code::parse(CODE).expect("digits");
    let first = origin(&opened.session_id);
    let redeemed = sessions
        .verify(&opened.session_id, &code, &first, now())
        .await
        .expect("verify");
    assert!(!redeemed.repeated);

    // The same request again, inside the window: the same envelope, flagged as
    // a repeat for the audit trail and identical on the wire.
    let again = sessions
        .verify(&opened.session_id, &code, &first, now())
        .await
        .expect("replay");
    assert!(again.repeated);
    assert_eq!(again.ciphertext, redeemed.ciphertext);

    // Anybody else asking is told it is gone.
    let stranger = Fingerprint::of("198.51.100.4", "curl/8", &opened.session_id);
    let refusal = sessions
        .verify(&opened.session_id, &code, &stranger, now())
        .await
        .expect_err("consumed");
    assert_eq!(refusal.code(), error_code::SESSION_CONSUMED);
}

/// Dimension 1.3 — verifying before approval never advances the machine.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_device_flow_verify_before_approve_leaves_it_approvable() {
    let harness = RedisHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    let code = Code::parse(CODE).expect("digits");
    let fingerprint = origin(&opened.session_id);

    for _ in 0..5 {
        let refusal = sessions
            .verify(&opened.session_id, &code, &fingerprint, now())
            .await
            .expect_err("not approved");
        assert_eq!(refusal.code(), error_code::SESSION_NOT_APPROVED);
    }

    // Five early attempts must not have spent the attempt ceiling: no code was
    // ever checked, so nothing was wrong to count.
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("still approvable");
    sessions
        .verify(&opened.session_id, &code, &fingerprint, now())
        .await
        .expect("redeems normally");
}

/// Dimension 1.3 — the attempt ceiling is terminal, and says so distinctly.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_device_flow_attempt_ceiling_aborts_the_session() {
    let harness = RedisHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("approve");

    let wrong = Code::parse("000000").expect("digits");
    let fingerprint = origin(&opened.session_id);
    // Four rejections, then the fifth trips the ceiling. The count is the
    // store's; what this pins is that the LAST one reads differently from the
    // ones before it, so a command line stops prompting.
    for _ in 0..4 {
        let refusal = sessions
            .verify(&opened.session_id, &wrong, &fingerprint, now())
            .await
            .expect_err("rejected");
        assert_eq!(refusal.code(), error_code::VERIFICATION_FAILED);
    }
    let refusal = sessions
        .verify(&opened.session_id, &wrong, &fingerprint, now())
        .await
        .expect_err("ceiling");
    assert_eq!(refusal.code(), error_code::SESSION_ABORTED);

    // And the right code no longer works, because the session is gone.
    let refusal = sessions
        .verify(
            &opened.session_id,
            &Code::parse(CODE).expect("digits"),
            &fingerprint,
            now(),
        )
        .await
        .expect_err("aborted");
    assert_eq!(refusal.code(), error_code::SESSION_ABORTED);
}

/// Dimension 1.2 — one person's bulk cancel touches nobody else's logins.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_device_flow_bulk_cancel_is_scoped_to_its_owner() {
    let harness = RedisHarness::connect().await;
    let sessions = surface(&harness);
    let owner = format!("{APPROVER}_{}", std::process::id());
    let other = format!("{owner}_other");

    let mut mine = Vec::new();
    for _ in 0..3 {
        let opened = sessions
            .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
            .await
            .expect("open");
        sessions
            .approve(&opened.session_id, &approval(), &owner, now())
            .await
            .expect("approve");
        mine.push(opened.session_id);
    }
    let theirs = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    sessions
        .approve(&theirs.session_id, &approval(), &other, now())
        .await
        .expect("approve");

    let aborted = sessions.cancel_all(&owner).await.expect("cancel all");
    for session_id in &mine {
        assert!(
            aborted.contains(session_id),
            "{session_id} belongs to the owner and must be reported aborted"
        );
        let refusal = sessions.poll(session_id).await.expect_err("aborted");
        assert_eq!(refusal.code(), error_code::SESSION_ABORTED);
    }
    assert!(
        !aborted.contains(&theirs.session_id),
        "another person's login must survive a bulk cancel"
    );
    let survivor = sessions.poll(&theirs.session_id).await.expect("still live");
    assert_eq!(survivor.status, SessionStatus::VerificationPending);
}
