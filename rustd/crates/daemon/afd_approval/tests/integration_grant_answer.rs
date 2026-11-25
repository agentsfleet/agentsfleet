//! A person's answer to an access card: what the grant becomes, and the answer
//! turn that resumes the run that asked.
//!
//! `RESOLVE_GATE` answers the card and moves its grant in one statement, and
//! the resolve then lands the answer turn a question's answer lands. The card
//! these tests answer is the one the request verb raised, so the join under
//! test is the join against the row production writes.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_api_wire::grant::status;
use afd_approval::{Decision, GrantKind, Requested, Resolution};
use afd_turns::Producer;
use serde_json::json;

use crate::grant_lane::{
    GITHUB, READ_SCOPE, SENTRY, WRITE_SCOPE, answer, answer_turn, ask, cards, grant, only_card,
};
use crate::lane::Lane;

/// Approve: the grant is approved with the scope asked for, and exactly one
/// answer turn tells the waiting run `approved`; a retried resolve lands none.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_approved_card_lands_one_answer_turn() {
    let lane = Lane::isolated().await;
    let turn = lane.asking_turn().await;
    ask(
        &lane,
        GITHUB,
        GrantKind::Integration,
        Some(READ_SCOPE),
        &turn,
    )
    .await;
    let card = only_card(&lane).await;

    let first = answer(&lane, &card.action, Decision::Approved).await;

    let Resolution::Resolved(won) = first else {
        unreachable!("the first answer wins, got {first:?}");
    };
    let landed = won
        .continuation_turn_id
        .expect("an approved card answers the run that asked");
    let held = grant(&lane, GITHUB).await;
    assert_eq!(held.status, status::APPROVED);
    assert_eq!(
        held.scope,
        Some(json!({"repositories": ["org/app"], "access": "read"}))
    );
    assert_eq!(
        held.requested_scope, None,
        "the ask is in force, not waiting"
    );
    let resolved_at = only_card(&lane).await.resolved_at;
    assert_eq!(
        held.approved_at, resolved_at,
        "one statement answered the card and approved the grant"
    );
    assert_eq!(
        answer_turn(&lane, &landed).await,
        (
            json!({"message": "approved", "asked_turn_id": turn.as_str()}),
            Some(turn.to_string())
        )
    );

    let retried = answer(&lane, &card.action, Decision::Approved).await;
    let Resolution::AlreadyResolved(standing) = retried else {
        unreachable!("a retry reads the standing answer, got {retried:?}");
    };
    assert_eq!(standing.continuation_turn_id, Some(landed));
    assert_eq!(
        lane.continuations(Producer::QuestionAnswer, &card.action)
            .await,
        1,
        "one answer turn, however often the card is answered"
    );
}

/// Deny: the grant is revoked, and the answer turn tells the waiting run
/// `denied`, so it resumes knowing it holds nothing.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_denied_card_lands_a_denied_answer_and_revokes_the_grant() {
    let lane = Lane::isolated().await;
    let turn = lane.asking_turn().await;
    ask(&lane, SENTRY, GrantKind::Secret, None, &turn).await;
    let card = only_card(&lane).await;

    let outcome = answer(&lane, &card.action, Decision::Denied).await;

    let Resolution::Resolved(won) = outcome else {
        unreachable!("the first answer wins, got {outcome:?}");
    };
    let landed = won
        .continuation_turn_id
        .expect("a denied card answers the run that asked");
    let held = grant(&lane, SENTRY).await;
    assert_eq!(held.status, status::REVOKED);
    assert_eq!((held.scope, held.requested_scope), (None, None));
    assert_eq!(held.approved_at, None);
    assert_eq!(held.revoked_at, only_card(&lane).await.resolved_at);
    assert_eq!(
        answer_turn(&lane, &landed).await,
        (
            json!({"message": "denied", "asked_turn_id": turn.as_str()}),
            Some(turn.to_string())
        )
    );
}

/// A wider scope on an approved grant waits on its own card: the approved
/// scope stays in force until a person approves the wider one.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_wider_scope_keeps_the_approved_scope_until_approved() {
    let lane = Lane::isolated().await;
    let read_turn = lane.asking_turn().await;
    ask(
        &lane,
        GITHUB,
        GrantKind::Integration,
        Some(READ_SCOPE),
        &read_turn,
    )
    .await;
    answer(&lane, &only_card(&lane).await.action, Decision::Approved).await;
    let write_turn = lane.asking_turn().await;

    let widened = ask(
        &lane,
        GITHUB,
        GrantKind::Integration,
        Some(WRITE_SCOPE),
        &write_turn,
    )
    .await;

    let read = json!({"repositories": ["org/app"], "access": "read"});
    let write = json!({"repositories": ["org/app"], "access": "write", "base": "main"});
    let waiting = grant(&lane, GITHUB).await;
    let wider = cards(&lane)
        .await
        .pop()
        .expect("the wider ask raised a card");
    assert_eq!(
        widened,
        Requested::Raised {
            grant: waiting.id.clone(),
            gate: wider.gate.clone()
        }
    );
    assert_eq!(waiting.status, status::APPROVED);
    assert_eq!(waiting.scope, Some(read.clone()), "read stays in force");
    assert_eq!(waiting.requested_scope, Some(write.clone()));
    assert_eq!(wider.turn, Some(write_turn));

    answer(&lane, &wider.action, Decision::Approved).await;

    let held = grant(&lane, GITHUB).await;
    assert_eq!(held.status, status::APPROVED);
    assert_eq!(held.scope, Some(write), "the approved ask is now in force");
    assert_eq!(held.requested_scope, None);
}

/// A denied wider scope revokes the grant: a card's answer is what its grant
/// becomes, and a no is never read as "keep the narrower yes".
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_denied_wider_scope_revokes_the_grant() {
    let lane = Lane::isolated().await;
    let read_turn = lane.asking_turn().await;
    ask(
        &lane,
        GITHUB,
        GrantKind::Integration,
        Some(READ_SCOPE),
        &read_turn,
    )
    .await;
    answer(&lane, &only_card(&lane).await.action, Decision::Approved).await;
    let write_turn = lane.asking_turn().await;
    ask(
        &lane,
        GITHUB,
        GrantKind::Integration,
        Some(WRITE_SCOPE),
        &write_turn,
    )
    .await;
    let wider = cards(&lane)
        .await
        .pop()
        .expect("the wider ask raised a card");

    answer(&lane, &wider.action, Decision::Denied).await;

    let held = grant(&lane, GITHUB).await;
    assert_eq!(held.status, status::REVOKED);
    assert_eq!(held.requested_scope, None);
    assert!(held.revoked_at.is_some(), "the deny stamps the revoke");
}
