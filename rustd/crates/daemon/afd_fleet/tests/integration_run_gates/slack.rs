//! One Slack request is one attempt: however often Slack delivers a mention,
//! the write-bound fleet it reaches is held once, on a branch named for that
//! event, with no approval asked for.
//!
//! Split from the binding cases beside it. The events arrive through the
//! turn ledger under the Slack producer and its key, as the mention route
//! admits them, rather than straight onto the stream: the ledger's key is what
//! makes a redelivery the same event, and so the same branch.

use af_wire::turn::TurnKind;
use afd_gate::policy::repair;
use afd_turns::{Admitted, IncomingTurn, Key, Producer, Turns};

use super::seed::seed_provider_resolution;
use super::*;
use crate::recovery_seed::{incoming_turn, ledger};

/// One Slack mention, admitted as the mention route admits it.
async fn admit_mention(ledger: &Turns, fleet: &Uuid7, workspace: &Uuid7, key: &str) -> Admitted {
    ledger
        .admit(IncomingTurn {
            producer: Producer::SlackMention,
            key: Key::Repeated(key),
            event_type: TurnKind::Chat,
            ..incoming_turn(fleet, workspace, key)
        })
        .await
        .expect("the ledger admits the mention")
}

/// Dimension 3.3 — Slack's retry of one mention is the same event, so it puts
/// nothing more on the stream and the fleet is held once for it, on a branch
/// named for it and with no approval asked for; a second mention is a second
/// event with a branch of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn one_slack_request_is_one_attempt() {
    crate::support::install_subscriber();
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    set_config(&fixtures, &fleet, WRITE_BOUND_CONFIG).await;
    let _default = seed_provider_resolution(&fixtures, &fleet).await;

    let ledger = ledger(&fixtures);
    let (first, second) = two_mentions(&ledger, &fleet, &workspace).await;

    // One run: the fleet runs one event at a time, so the first is the one
    // held, on the branch named for it, and no approval is asked for.
    let ready = Ready {
        runner,
        fleet: fleet.clone(),
        turn_id: first.stored.id.clone(),
        tenant,
    };
    let assigned = assign(&fixtures, &ready).await;
    let answer = drive(&fixtures, &ready, assigned).await;
    assert!(
        answer.is_some(),
        "a Slack-requested write event was held rather than held: {answer:?}"
    );
    let first_branch = repair::branch_for(first.stored.id.as_str());
    assert!(
        answer
            .as_ref()
            .is_some_and(|run| run.to_string().contains(&first_branch)),
        "{first_branch}: {answer:?}"
    );

    let mut connection = fixtures.database.acquire().await.expect("database");
    let states: Vec<(bool,)> = sqlx::query_as(
        "SELECT run_fence IS NOT NULL FROM thread.turns WHERE thread_id = $1 ORDER BY created_at, sequence"
    ).bind(&fleet).fetch_all(&mut *connection).await.expect("queue state");
    assert_eq!(states, [(true,), (false,)], "one delivered, one waiting");
    drop(connection);
    // The second request authors elsewhere: its branch is named for its own
    // event, the name the run path gives it (`deliver.rs`).
    assert_ne!(repair::branch_for(second.stored.id.as_str()), first_branch);

    fixtures.cleanup().await;
}

async fn two_mentions(ledger: &Turns, fleet: &Uuid7, workspace: &Uuid7) -> (Admitted, Admitted) {
    // Keyed off the minted fleet, so no two runs of the suite share a key.
    let first_key = format!("{fleet}:EvSlack01");
    let first = admit_mention(ledger, fleet, workspace, &first_key).await;
    let retried = admit_mention(ledger, fleet, workspace, &first_key).await;
    assert_eq!(
        retried.stored.id, first.stored.id,
        "a retry is the same event"
    );
    assert!(retried.replayed);
    let second = admit_mention(ledger, fleet, workspace, &format!("{fleet}:EvSlack02")).await;
    assert_ne!(
        second.stored.id, first.stored.id,
        "a second mention is a second event"
    );

    (first, second)
}
