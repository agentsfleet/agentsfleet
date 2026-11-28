//! A fleet's access request: the grant row and the one card a person answers.
//!
//! # Why these need a real datastore
//!
//! The grant and its card share a transaction, the grant row is unique per
//! fleet, kind and name, and the unique active grant reference arbitrates cards.
//! Each claim here is a claim about what those statements and constraints
//! decide, so each is proven against the real schema.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_api_wire::approval::status as card_status;
use afd_api_wire::grant::status;
use afd_approval::{Decision, GrantKind, REASON_ASKED_BY_THE_FLEET, Requested};
use serde_json::json;
use tokio::sync::Barrier;

use crate::grant_lane::{
    GITHUB, READ_SCOPE, SENTRY, answer, ask, cards, grant, grant_count, only_card,
};
use crate::lane::Lane;

/// Callers released together on one fresh grant.
const CALLERS: usize = 100;

/// A first request writes the grant pending and raises one card carrying the
/// asking turn and the scope asked for.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_first_request_writes_a_pending_grant_and_one_card_on_its_turn() {
    let lane = Lane::isolated().await;
    let turn = lane.asking_turn().await;

    let outcome = ask(
        &lane,
        GITHUB,
        GrantKind::Integration,
        Some(READ_SCOPE),
        &turn,
    )
    .await;

    let held = grant(&lane, GITHUB).await;
    let card = only_card(&lane).await;
    assert_eq!(
        outcome,
        Requested::Raised {
            grant: held.id.clone(),
            gate: card.gate.clone()
        }
    );
    assert_eq!(
        (
            held.grant_type.as_str(),
            held.status.as_str(),
            held.reason.as_str()
        ),
        ("integration", status::PENDING, REASON_ASKED_BY_THE_FLEET)
    );
    assert_eq!(
        held.scope, None,
        "nothing is in force until a person approves"
    );
    assert_eq!(
        held.requested_scope,
        Some(json!({"repositories": ["org/app"], "access": "read"}))
    );
    assert_eq!(
        card.turn,
        Some(turn),
        "the card carries the turn that asked"
    );
    assert_eq!(
        card.evidence,
        json!({
            "service": "github",
            "kind": "integration",
            "scope": {"repositories": ["org/app"], "access": "read"},
        })
    );
    assert_eq!(card.status, card_status::PENDING);
}

/// A second request while a card is open raises none beside it, and names the
/// open card.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_second_request_while_pending_raises_no_second_card() {
    let lane = Lane::isolated().await;
    let first_turn = lane.asking_turn().await;
    let second_turn = lane.asking_turn().await;

    let first = ask(&lane, SENTRY, GrantKind::Secret, None, &first_turn).await;
    let second = ask(&lane, SENTRY, GrantKind::Secret, None, &second_turn).await;

    let card = only_card(&lane).await;
    let held = grant(&lane, SENTRY).await;
    assert!(matches!(first, Requested::Raised { .. }), "{first:?}");
    assert_eq!(
        second,
        Requested::Pending {
            grant: held.id,
            gate: Some(card.gate)
        }
    );
    assert_eq!(card.turn, Some(first_turn), "the open card keeps its turn");
    assert_eq!(grant_count(&lane).await, 1);
}

/// A request the approved grant already holds is covered: no card, no write.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_covered_request_writes_nothing() {
    let lane = Lane::isolated().await;
    for (name, kind, scope) in [
        (SENTRY, GrantKind::Secret, None),
        (GITHUB, GrantKind::Integration, Some(READ_SCOPE)),
    ] {
        let turn = lane.asking_turn().await;
        ask(&lane, name, kind, scope, &turn).await;
        let raised = cards(&lane).await.last().expect("a card").action.clone();
        answer(&lane, &raised, Decision::Approved).await;
        let approved = grant(&lane, name).await;
        let cards_before = cards(&lane).await.len();

        let again = lane.asking_turn().await;
        let outcome = ask(&lane, name, kind, scope, &again).await;

        assert_eq!(
            outcome,
            Requested::Covered {
                grant: approved.id.clone()
            },
            "{name}"
        );
        assert_eq!(
            grant(&lane, name).await,
            approved,
            "{name}: the row is untouched"
        );
        assert_eq!(cards(&lane).await.len(), cards_before, "{name}: no card");
    }
}

/// A denied name can be asked again: the grant moves back to pending and one
/// new card is raised, while the denied card stays answered.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_request_after_a_deny_raises_a_new_card() {
    let lane = Lane::isolated().await;
    let first_turn = lane.asking_turn().await;
    ask(&lane, SENTRY, GrantKind::Secret, None, &first_turn).await;
    let denied = only_card(&lane).await;
    answer(&lane, &denied.action, Decision::Denied).await;
    assert_eq!(grant(&lane, SENTRY).await.status, status::REVOKED);

    let second_turn = lane.asking_turn().await;
    let outcome = ask(&lane, SENTRY, GrantKind::Secret, None, &second_turn).await;

    let held = grant(&lane, SENTRY).await;
    let all = cards(&lane).await;
    let [old, new] = all.as_slice() else {
        unreachable!("one denied card and one new card: {all:?}");
    };
    assert_eq!(
        outcome,
        Requested::Raised {
            grant: held.id.clone(),
            gate: new.gate.clone()
        }
    );
    assert_eq!(held.status, status::PENDING);
    assert_eq!((held.approved_at, held.revoked_at), (None, None));
    assert_eq!(old.gate, denied.gate);
    assert_eq!(old.status, card_status::DENIED);
    assert_eq!(new.status, card_status::PENDING);
    assert_eq!(
        new.turn,
        Some(second_turn),
        "the new card carries the new ask"
    );
    assert_eq!(grant_count(&lane).await, 1, "one grant row per name");
}

/// A hundred requests released together on a fresh name raise one card.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_hundred_concurrent_requests_raise_one_card() {
    let lane = Arc::new(Lane::isolated().await);
    let turn = lane.asking_turn().await;
    let barrier = Arc::new(Barrier::new(CALLERS));
    let callers: Vec<_> = (0..CALLERS)
        .map(|_caller| {
            let (lane, barrier, turn) = (Arc::clone(&lane), Arc::clone(&barrier), turn.clone());
            tokio::spawn(async move {
                barrier.wait().await;
                ask(&lane, SENTRY, GrantKind::Secret, None, &turn).await
            })
        })
        .collect();
    let mut outcomes = Vec::with_capacity(CALLERS);
    for caller in callers {
        outcomes.push(caller.await.expect("the caller joins"));
    }

    let raised = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, Requested::Raised { .. }))
        .count();
    let pending = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, Requested::Pending { .. }))
        .count();
    assert_eq!((raised, pending), (1, CALLERS - 1), "{outcomes:?}");
    assert_eq!(cards(&lane).await.len(), 1);
    assert_eq!(grant_count(&lane).await, 1);
}

/// One fleet's request is not another's.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_request_names_the_fleet_that_asked() {
    let mine = Lane::isolated().await;
    let theirs = Lane::isolated().await;
    let turn = mine.asking_turn().await;

    ask(&mine, SENTRY, GrantKind::Secret, None, &turn).await;

    assert_eq!(grant_count(&theirs).await, 0);
    assert_eq!(cards(&theirs).await, Vec::new());
    assert_eq!(grant_count(&mine).await, 1);
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_host_and_a_credential_of_one_name_stay_two_grants() {
    use crate::grant_lane::grants;
    use afd_approval::Revocation;
    let lane = Lane::isolated().await;
    let host = "support.vendor.example";
    let first = lane.asking_turn().await;
    let second = lane.asking_turn().await;
    let secret_turn = lane.asking_turn().await;
    let (a, b) = tokio::join!(
        ask(&lane, host, GrantKind::Host, None, &first),
        ask(&lane, host, GrantKind::Host, None, &second)
    );
    assert!(matches!(
        (&a, &b),
        (Requested::Raised { .. }, Requested::Pending { .. })
            | (Requested::Pending { .. }, Requested::Raised { .. })
    ));
    let asked_secret = ask(&lane, host, GrantKind::Secret, None, &secret_turn).await;
    assert_ne!(a.grant(), asked_secret.grant());
    let open = cards(&lane).await;
    assert_eq!(open.len(), 2);
    let host_card = open
        .iter()
        .find(|c| c.evidence.get("kind").and_then(serde_json::Value::as_str) == Some("host"))
        .expect("the host card");
    let secret_card = open
        .iter()
        .find(|c| c.evidence.get("kind").and_then(serde_json::Value::as_str) == Some("secret"))
        .expect("the credential card");
    answer(&lane, &host_card.action, Decision::Approved).await;
    assert!(matches!(
        ask(&lane, host, GrantKind::Host, None, &second).await,
        Requested::Covered { .. }
    ));
    assert!(matches!(
        ask(&lane, host, GrantKind::Secret, None, &secret_turn).await,
        Requested::Pending { .. }
    ));
    answer(&lane, &secret_card.action, Decision::Approved).await;
    let store = grants(&lane);
    assert_eq!(
        store
            .revoke(&lane.workspace, &lane.fleet, a.grant())
            .await
            .expect("host revocation"),
        Revocation::Revoked
    );
    assert!(matches!(
        ask(&lane, host, GrantKind::Secret, None, &secret_turn).await,
        Requested::Covered { .. }
    ));
}
