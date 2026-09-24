//! A report owes its answer only to the destination its event was admitted
//! with, and never to the lease's model provider.
//!
//! Driven end to end through the real verbs: `Admissions::admit` records the
//! event and its destination, the lease path takes it, and `commit_report`
//! settles it. What is read back is `core.fleet_obligations` for this test's
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

use std::borrow::Cow;

use afd_admission::{Producer, Reply};
use afd_connector::Provider;
use afd_core::clock::UnixMillis;
use afd_fleet::lease::Committed;
use afd_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};

use crate::report_commit::{
    RESPONSE_ACCEPTED, RESPONSE_POSTGRES_REFUSES, RESUME_EVENT_ID, RESUME_RESPONSE,
};
use crate::report_seed::SLICE_MS;
use crate::requests::ENROLLED_AT;
use crate::seed::PROVIDER;

#[path = "integration_report_owes_destination/stage.rs"]
mod stage;

use self::stage::{Obligation, obligations, owed_by, stage};

/// The thread the asking event was admitted with.
const THREAD: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;

/// Dimension 2.1 — non-empty answers from a steer, an App webhook and a cron
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

/// Dimension 2.2 — an event admitted with a Slack thread owes one row to that
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

    let owing = owed_by(stage.run_next(0).await).expect("an answer asked from a thread is owed");
    assert_eq!(owing.reply.provider, Provider::Slack);
    assert_eq!(owing.reply.address, THREAD);
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

/// Dimension 2.3 — with every lease resolved to a model provider, no ledger
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

/// Dimension 2.4 — a report the datastore refuses owes nothing, and a replayed
/// report owes once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_replayed_or_rolled_back_report_owes_at_most_once() {
    let stage = stage().await;
    stage
        .admit(
            Producer::Webhook,
            "asked-once",
            Reply::To {
                connector: Provider::Slack.id(),
                address: THREAD,
            },
        )
        .await;
    let now = UnixMillis::from_millis(ENROLLED_AT);
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

/// An event id the admission ledger never minted.
const PRE_LEDGER_EVENT: &str = "evt_predates_the_ledger";

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
    let now = UnixMillis::from_millis(ENROLLED_AT);
    let (lease_id, mut lease) = stage.lease_next(now).await;
    lease.event_id = PRE_LEDGER_EVENT.to_owned();

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
    let event_id = stage
        .admit(
            Producer::Webhook,
            "asked-and-answered",
            Reply::To {
                connector: Provider::Slack.id(),
                address: THREAD,
            },
        )
        .await;
    let now = UnixMillis::from_millis(ENROLLED_AT);
    let (lease_id, lease) = stage.lease_next(now).await;

    let request = ReportRequest {
        lease_id: Cow::Borrowed(&lease_id),
        event_id: Cow::Borrowed(&lease.event_id),
        fencing_token: lease.fence.as_u64(),
        outcome: Outcome::Processed,
        failure_reason: None,
        failure_detail: Cow::Borrowed(""),
        response_text: Cow::Borrowed(RESPONSE_ACCEPTED),
        tokens: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: u64::try_from(SLICE_MS).expect("a slice is positive"),
        },
        checkpoint: ReportCheckpoint {
            last_event_id: Cow::Borrowed(RESUME_EVENT_ID),
            last_response: Cow::Borrowed(RESUME_RESPONSE),
        },
    };
    stage
        .fixtures
        .plane()
        .report(&stage.runner, &request, now.saturating_add_millis(SLICE_MS))
        .await
        .expect("the plane settles the report");

    let mut connection = stage
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let receipt: Option<String> = sqlx::query_scalar(
        "SELECT receipt FROM core.fleet_obligations
          WHERE fleet_id = $1::uuid AND event_id = $2",
    )
    .bind(&stage.fleet)
    .bind(&event_id)
    .fetch_one(&mut *connection)
    .await
    .expect("the report owed the thread's answer");
    assert!(
        receipt.is_some(),
        "the plane appended the owed answer and recorded the entry it landed on"
    );
    drop(connection);

    stage.fixtures.cleanup().await;
}
