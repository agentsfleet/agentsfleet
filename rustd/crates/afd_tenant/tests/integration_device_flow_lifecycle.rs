//! §1 — a device-flow login's terminal states and its cancels, against a real
//! queue: a redeemed session stays gone, and a cancel reaches its owner's
//! logins and nobody else's.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::error_code;
use afd_tenant::session::SessionStatus;
use afd_tenant::session::input::{Code, Opening};

use crate::device_flow_lane::{CODE, NOW, approval, origin, owner, surface};
use crate::dragonfly_harness::DragonflyHarness;

/// Dimension 1.2 — a redeemed session is terminal, and reads as gone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_device_flow_terminal_states_are_terminal() {
    let harness = DragonflyHarness::connect().await;
    let sessions = surface(&harness);
    let approver = owner("terminal");

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), NOW)
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), &approver, NOW)
        .await
        .expect("approve");
    sessions
        .verify(
            &opened.session_id,
            &Code::parse(CODE).expect("digits"),
            &origin(&opened.session_id),
            NOW,
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

    // And a bulk abort by its owner cannot un-redeem it.
    let aborted = sessions.cancel_all(&approver).await.expect("cancel all");
    assert!(!aborted.contains(&opened.session_id), "{aborted:?}");
    let refusal = sessions
        .poll(&opened.session_id)
        .await
        .expect_err("consumed");
    assert_eq!(refusal.code(), error_code::SESSION_CONSUMED);
}

/// A login is aborted by its owner's bulk cancel and by nobody else's.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_device_flow_cancel_is_owner_checked_and_idempotent() {
    let harness = DragonflyHarness::connect().await;
    let sessions = surface(&harness);
    let approver = owner("idempotent");

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), NOW)
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), &approver, NOW)
        .await
        .expect("approve");

    let foreign = sessions
        .cancel_all("user_someone_else")
        .await
        .expect("cancel all");
    assert!(!foreign.contains(&opened.session_id), "{foreign:?}");

    let first = sessions.cancel_all(&approver).await.expect("cancel all");
    assert!(
        first.contains(&opened.session_id),
        "the owner's cancel performs the transition: {first:?}"
    );
    let repeat = sessions.cancel_all(&approver).await.expect("cancel all");
    assert!(
        !repeat.contains(&opened.session_id),
        "a repeat changes nothing, so nothing is audited twice: {repeat:?}"
    );

    let refusal = sessions
        .poll(&opened.session_id)
        .await
        .expect_err("aborted");
    assert_eq!(refusal.code(), error_code::SESSION_ABORTED);
}

/// Dimension 1.2 — one person's bulk cancel touches nobody else's logins.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_device_flow_bulk_cancel_is_scoped_to_its_owner() {
    let harness = DragonflyHarness::connect().await;
    let sessions = surface(&harness);
    let owner = owner("bulk");
    let other = format!("{owner}_other");

    let mut mine = Vec::new();
    for _ in 0..3 {
        let opened = sessions
            .open(&Opening::parse("cli-key", "laptop").expect("bounds"), NOW)
            .await
            .expect("open");
        sessions
            .approve(&opened.session_id, &approval(), &owner, NOW)
            .await
            .expect("approve");
        mine.push(opened.session_id);
    }
    let theirs = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), NOW)
        .await
        .expect("open");
    sessions
        .approve(&theirs.session_id, &approval(), &other, NOW)
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
