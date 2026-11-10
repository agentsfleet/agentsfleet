//! A report owes its answer only to the destination its event was admitted
//! with, and never to the lease's model provider.
//!
//! Driven end to end through the real verbs: `Admissions::admit` records the
//! event and its destination, the lease path takes it, and `commit_report`
//! settles it. What is read back is `fleet.obligations` for this test's
//! own fleet (ISO-1). No suite drove a report into that table before this one,
//! which is how every non-empty answer came to be owed to `anthropic` unseen.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without datastores, and `make test-integration-rustd` is the only lane that
//! executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Producer, Reply};
use afd_connector::Provider;
use afd_db::test_util::mint_id;
use afd_fleet::lease::Committed;

use crate::report_commit::{RESPONSE_ACCEPTED, RESPONSE_POSTGRES_REFUSES};
use crate::seed::PROVIDER;

pub(crate) mod stage;

use self::stage::{Obligation, obligations, owed_by, stage};

/// The thread the asking event was admitted with.
const THREAD: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;

/// Non-empty answers from a steer, an App webhook and a cron
/// fire owe nothing: none of them was asked from a thread.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn answers_without_a_destination_owe_nothing() {
    let stage = stage().await;
    for producer in [
        Producer::Steer,
        Producer::WebhookApp,
        Producer::ScheduleFire,
    ] {
        stage.admit(producer, producer.as_str(), Reply::None).await;
    }

    for run in 0..3 {
        assert_eq!(
            owed_by(stage.run_next(run).await),
            None,
            "run {run} answered, and nobody asked from anywhere an answer can go"
        );
    }
    assert_eq!(
        obligations(&stage.fixtures, &stage.fleet).await,
        Vec::new(),
        "three non-empty answers, no destination, no obligation"
    );

    stage.fixtures.cleanup().await;
}

/// an event admitted with a Slack thread owes one row to that
/// connector and that exact address.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn answer_is_owed_to_the_recorded_destination() {
    let stage = stage().await;
    let event_id = stage
        .admit(
            Producer::Webhook,
            "asked-in-a-thread",
            Reply::To {
                connector: Provider::Slack.id(),
                address: THREAD,
            },
        )
        .await;

    owed_by(stage.run_next(0).await).expect("an answer asked from a thread is owed");
    assert_eq!(
        obligations(&stage.fixtures, &stage.fleet).await,
        vec![Obligation {
            provider: Provider::Slack.id().to_owned(),
            destination: Some(THREAD.to_owned()),
            event_id,
        }],
        "one row, owed to the thread the question came from"
    );

    stage.fixtures.cleanup().await;
}

/// with every lease resolved to a model provider, no ledger
/// row names it, whichever producer asked.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_model_provider_never_reaches_the_ledger() {
    let stage = stage().await;
    stage.admit(Producer::Steer, "steer", Reply::None).await;
    stage
        .admit(
            Producer::Webhook,
            "thread",
            Reply::To {
                connector: Provider::Slack.id(),
                address: THREAD,
            },
        )
        .await;
    stage
        .admit(Producer::ScheduleFire, "schedule", Reply::None)
        .await;

    for run in 0..3 {
        stage.run_next(run).await;
    }
    let owed = obligations(&stage.fixtures, &stage.fleet).await;
    assert_eq!(
        owed.len(),
        1,
        "only the thread's question is owed: {owed:?}"
    );
    assert!(
        owed.iter().all(|row| row.provider != PROVIDER),
        "the lease's model provider ({PROVIDER}) reached the delivery ledger: {owed:?}"
    );

    stage.fixtures.cleanup().await;
}

/// a report the datastore refuses owes nothing, and a replayed
/// report owes once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_replayed_or_rolled_back_report_owes_at_most_once() {
    let stage = stage().await;
    stage.admit_thread("asked-once").await;
    let now = stage.fixtures.enrolled_at();
    let (lease_id, lease) = stage.lease_next(now).await;

    let refused = stage
        .report(&lease_id, &lease, RESPONSE_POSTGRES_REFUSES, now)
        .await;
    assert!(
        refused.is_err(),
        "an answer Postgres will not store fails the report"
    );
    assert_eq!(
        obligations(&stage.fixtures, &stage.fleet).await,
        Vec::new(),
        "the refused report rolled back, and the obligation with it"
    );

    let settled = stage
        .report(&lease_id, &lease, RESPONSE_ACCEPTED, now)
        .await
        .expect("the retry must reach the datastore");
    assert!(
        owed_by(settled).is_some(),
        "the retry that settled owes the answer"
    );
    let repeated = stage
        .report(&lease_id, &lease, RESPONSE_ACCEPTED, now)
        .await
        .expect("a repeated report is an answer, not a fault");
    assert!(
        matches!(repeated, Committed::AlreadySettled),
        "the lease is settled, so the repeat writes nothing"
    );
    assert_eq!(
        obligations(&stage.fixtures, &stage.fleet).await.len(),
        1,
        "a refused, a settled and a repeated report owe one delivery between them"
    );

    stage.fixtures.cleanup().await;
}

/// A connector id nothing in the catalogue answers to, as a producer bug or an
/// edit made out of band would record it.
const UNKNOWN_CONNECTOR: &str = "carrier-pigeon";

/// A recorded connector no connector answers to owes nothing: the report reads
/// it, cannot parse it, and settles without an obligation.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_unknown_recorded_connector_owes_nothing() {
    let stage = stage().await;
    stage
        .admit(
            Producer::Webhook,
            "asked-through-nothing",
            Reply::To {
                connector: UNKNOWN_CONNECTOR,
                address: THREAD,
            },
        )
        .await;

    assert_eq!(
        owed_by(stage.run_next(0).await),
        None,
        "a destination no poster can take is owed nothing"
    );
    assert_eq!(obligations(&stage.fixtures, &stage.fleet).await, Vec::new());

    stage.fixtures.cleanup().await;
}

/// A lease over an event id the ledger never minted owes nothing, and still
/// settles: the run happened and is charged for.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_event_the_ledger_never_minted_owes_nothing() {
    let stage = stage().await;
    stage
        .admit(
            Producer::Webhook,
            "asked-before-the-ledger",
            Reply::To {
                connector: Provider::Slack.id(),
                address: THREAD,
            },
        )
        .await;
    let now = stage.fixtures.enrolled_at();
    let (lease_id, mut lease) = stage.lease_next(now).await;
    // An event id the admission ledger never minted.
    lease.event_id = mint_id();

    let committed = stage
        .report(&lease_id, &lease, RESPONSE_ACCEPTED, now)
        .await
        .expect("the report must reach the datastore");
    assert_eq!(owed_by(committed), None, "no ledger row, no destination");
    assert_eq!(obligations(&stage.fixtures, &stage.fleet).await, Vec::new());

    stage.fixtures.cleanup().await;
}

/// The plane's report owes the answer and then appends and receipts it, so the
/// fast path — not the producer's recovery — is what carries a thread's answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_plane_appends_and_receipts_what_it_owed() {
    let stage = stage().await;
    let event_id = stage.admit_thread("asked-and-answered").await;
    let now = stage.fixtures.enrolled_at();
    let (lease_id, lease) = stage.lease_next(now).await;

    stage.report_through_plane(&lease_id, &lease, now).await;

    let receipt = stage.receipt(&event_id).await;
    assert!(
        receipt.is_some(),
        "the plane appended the owed answer and recorded the entry it landed on"
    );

    stage.fixtures.cleanup().await;
}
