//! One Slack request is one attempt: however often Slack delivers a mention,
//! the write-bound fleet it reaches is leased once, on a branch named for that
//! event, with no approval asked for.
//!
//! Split from the binding cases beside it. The events arrive through the
//! admission ledger under the Slack producer and its key, as the mention route
//! admits them, rather than straight onto the stream: the ledger's key is what
//! makes a redelivery the same event, and so the same branch.

use afd_admission::{Admission, Admissions, Admitted, Key, Producer};
use afd_dragonfly::FleetStreams;
use afd_gate::policy::repair;
use afd_wire::event::EventType;

use super::seed::seed_provider_resolution;
use super::*;
use crate::integration_admission_recovery::{admission, ledger};

/// One Slack mention, admitted as the mention route admits it.
async fn admit_mention(ledger: &Admissions, fleet: &str, workspace: &str, key: &str) -> Admitted {
    ledger
        .admit(Admission {
            producer: Producer::SlackMention,
            key: Key::Repeated(key),
            event_type: EventType::Chat,
            ..admission(fleet, workspace, key)
        })
        .await
        .expect("the ledger admits the mention")
}

/// Dimension 3.3 — Slack's retry of one mention is the same event, so it puts
/// nothing more on the stream and the fleet is leased once for it, on a branch
/// named for it and with no approval asked for; a second mention is a second
/// event with a branch of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn one_slack_request_is_one_attempt() {
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    set_config(&fixtures, &fleet, WRITE_BOUND_CONFIG).await;
    seed_provider_resolution(&fixtures, &fleet).await;

    // Keyed off the minted fleet, so no two runs of the suite share a key.
    let first_key = format!("{fleet}:EvSlack01");
    let ledger = ledger(&fixtures);
    let first = admit_mention(&ledger, &fleet, &workspace, &first_key).await;
    let retried = admit_mention(&ledger, &fleet, &workspace, &first_key).await;
    assert_eq!(retried.id, first.id, "a retry is the same event");
    assert!(retried.replayed);
    let second = admit_mention(&ledger, &fleet, &workspace, &format!("{fleet}:EvSlack02")).await;
    assert_ne!(second.id, first.id, "a second mention is a second event");

    // One lease: the fleet runs one event at a time, so the first is the one
    // leased, on the branch named for it, and no approval is asked for.
    let ready = Ready {
        runner,
        fleet: fleet.clone(),
        event_id: first.id.clone(),
        tenant,
    };
    let claimed = claim(&fixtures, &ready).await;
    let answer = drive(&fixtures, &ready, claimed).await;
    assert!(
        !answer.contains(NO_LEASE),
        "a Slack-requested write event was held rather than leased: {answer}"
    );
    let first_branch = repair::branch_for(&first.id);
    assert!(answer.contains(&first_branch), "{first_branch}: {answer}");

    // Three deliveries, two entries: the one leased and the one after it. The
    // retry put nothing on the stream, so it can never be a second attempt.
    let backlog = FleetStreams::new(fixtures.queue().clone())
        .backlog(&fleet)
        .await
        .expect("the backlog reads")
        .expect("the lease created the fleet's group");
    assert_eq!(
        (backlog.pending, backlog.undelivered),
        (1, Some(1)),
        "one entry leased, one waiting"
    );
    // The second request authors elsewhere: its branch is named for its own
    // event, the name the lease path gives it (`deliver.rs`).
    assert_ne!(repair::branch_for(&second.id), first_branch);

    fixtures.cleanup().await;
}
