//! Where an admission's answer goes: recorded by its producer, copied by a
//! continuation, and never half-written.
//!
//! Driven through `Admissions::admit` and read back with the shipped
//! `SELECT_REPLY_DESTINATION`, so a predicate added to the real lookup fails
//! here instead of passing against a copy. Each test mints its own fleet and
//! reads only that fleet's rows (ISO-1); none takes `RECOVERY_LANE`, because
//! none runs a deployment-wide sweep.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without datastores, and `make test-integration-rustd` is the only lane that
//! executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Admission, Admitted, Key, Producer, Reply, logical_parts, sql};
use afd_wire::event::EventType;
use sqlx::Row as _;

use crate::integration_admission_recovery::{admission, ledger, producer_key};
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// A connector id as a producer owning a reply surface states it.
const CONNECTOR: &str = "slack";

/// Two thread addresses that differ only in the thread.
const THREAD_A: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;
const THREAD_B: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000200"}"#;

/// The check slot 918 names.
const BOTH_OR_NEITHER: &str = "ck_fleet_admissions_reply_both_or_neither";

/// The index slot 918 adds, and what its definition must say.
const LOOKUP_INDEX: &str = "idx_fleet_admissions_reply_lookup";
const LOOKUP_COLUMNS: &str = "(fleet_id, created_at, seq)";
const LOOKUP_PREDICATE: &str = "WHERE (reply_provider IS NOT NULL)";

/// An id this ledger never minted.
const FOREIGN_EVENT: &str = "not-a-ledger-id";

/// Every producer that owns no reply surface, which is every one but the
/// continuation.
const NO_SURFACE: [Producer; 5] = [
    Producer::Steer,
    Producer::Webhook,
    Producer::WebhookApp,
    Producer::ScheduleFire,
    Producer::RepairVerification,
];

/// A webhook admission that states `reply`.
fn replying<'a>(
    fleet: &'a str,
    workspace: &'a str,
    key: &'a str,
    reply: Reply<'a>,
) -> Admission<'a> {
    Admission {
        reply,
        ..admission(fleet, workspace, key)
    }
}

/// The destination one event was admitted with, read by the shipped lookup.
async fn destination(fixtures: &Fixtures, fleet: &str, event_id: &str) -> Option<(String, String)> {
    let (created_at, seq) = logical_parts(event_id).expect("the ledger minted this id");
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(sql::SELECT_REPLY_DESTINATION)
        .bind(fleet)
        .bind(created_at)
        .bind(seq)
        .fetch_optional(&mut *connection)
        .await
        .expect("the lookup runs")
        .map(|row| {
            (
                row.try_get(0).expect("a connector is text"),
                row.try_get(1).expect("an address is text"),
            )
        })
}

/// Admits and answers the ledger's reply, failing loudly on a refusal.
async fn admit(fixtures: &Fixtures, admission: Admission<'_>) -> Admitted {
    ledger(fixtures)
        .admit(admission)
        .await
        .expect("the ledger admits")
}

/// Dimension 1.1 — a producer that owns no reply surface records none.
///
/// Every production call site states its `Reply` because the field has no
/// default; this proves what the ledger does with the answer they all give.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn existing_producers_record_no_reply_destination() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;

    for producer in NO_SURFACE {
        let key = producer_key(&fleet, producer.as_str());
        let admitted = admit(
            &fixtures,
            Admission {
                producer,
                key: Key::Repeated(&key),
                ..admission(&fleet, &workspace, &key)
            },
        )
        .await;
        assert_eq!(
            destination(&fixtures, &fleet, &admitted.id).await,
            None,
            "{producer:?} owns no reply surface and must record none"
        );
    }

    fixtures.cleanup().await;
}

/// Dimension 1.2 — a stated destination round-trips whole; half of one is
/// refused by the check.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn reply_destination_is_both_or_neither() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
    let key = producer_key(&fleet, "stated");
    let stated = Reply::To {
        connector: CONNECTOR,
        address: THREAD_A,
    };

    let admitted = admit(&fixtures, replying(&fleet, &workspace, &key, stated)).await;
    assert_eq!(
        destination(&fixtures, &fleet, &admitted.id).await,
        Some((CONNECTOR.to_owned(), THREAD_A.to_owned())),
        "the stated pair is stored exactly"
    );

    let (created_at, seq) = logical_parts(&admitted.id).expect("the ledger minted this id");
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let refused = sqlx::query(
        "UPDATE core.fleet_admissions SET reply_address = NULL \
         WHERE fleet_id = $1::uuid AND created_at = $2 AND seq = $3",
    )
    .bind(&fleet)
    .bind(created_at)
    .bind(seq)
    .execute(&mut *connection)
    .await
    .expect_err("a connector without an address must be refused");
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|failure| failure.constraint()),
        Some(BOTH_OR_NEITHER),
        "the refusal is the named both-or-neither check: {refused}"
    );

    fixtures.cleanup().await;
}

/// Dimension 1.3 — a continuation copies the destination of the event it
/// resumes, and copies nothing from an event that had none or that the ledger
/// never minted.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn continuation_inherits_the_resumed_destination() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;

    let asked_key = producer_key(&fleet, "asked-in-a-thread");
    let asked = admit(
        &fixtures,
        replying(
            &fleet,
            &workspace,
            &asked_key,
            Reply::To {
                connector: CONNECTOR,
                address: THREAD_A,
            },
        ),
    )
    .await;
    let silent_key = producer_key(&fleet, "asked-nowhere");
    let silent = admit(&fixtures, admission(&fleet, &workspace, &silent_key)).await;

    let resumed = [
        (
            asked.id.as_str(),
            Some((CONNECTOR.to_owned(), THREAD_A.to_owned())),
        ),
        (silent.id.as_str(), None),
        (FOREIGN_EVENT, None),
    ];
    for (event_id, expected) in resumed {
        let key = producer_key(&fleet, &format!("continues:{event_id}"));
        let continuation = admit(
            &fixtures,
            Admission {
                producer: Producer::GateContinuation,
                key: Key::Repeated(&key),
                event_type: EventType::Continuation,
                reply: Reply::Inherit { event_id },
                ..admission(&fleet, &workspace, &key)
            },
        )
        .await;
        assert_eq!(
            destination(&fixtures, &fleet, &continuation.id).await,
            expected,
            "a continuation of {event_id} answers where it would have"
        );
    }

    fixtures.cleanup().await;
}

/// Dimension 1.4 — a retried key naming another thread keeps the first
/// admission's, and is seen as drift.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn retried_key_with_a_new_destination_keeps_the_first() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
    let key = producer_key(&fleet, "retried");
    let first_reply = Reply::To {
        connector: CONNECTOR,
        address: THREAD_A,
    };
    let retry = replying(
        &fleet,
        &workspace,
        &key,
        Reply::To {
            connector: CONNECTOR,
            address: THREAD_B,
        },
    );

    let first = admit(&fixtures, replying(&fleet, &workspace, &key, first_reply)).await;
    let again = admit(&fixtures, retry).await;
    assert!(again.replayed, "the same key is the same event");
    assert_eq!(again.id, first.id, "the first call's id stands");
    assert_eq!(
        destination(&fixtures, &fleet, &first.id).await,
        Some((CONNECTOR.to_owned(), THREAD_A.to_owned())),
        "the first destination stands, as the first payload does"
    );

    let (created_at, seq) = logical_parts(&first.id).expect("the ledger minted this id");
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let stored: String = sqlx::query(
        "SELECT payload_digest FROM core.fleet_admissions \
         WHERE fleet_id = $1::uuid AND created_at = $2 AND seq = $3",
    )
    .bind(&fleet)
    .bind(created_at)
    .bind(seq)
    .fetch_one(&mut *connection)
    .await
    .expect("the row is there")
    .try_get(0)
    .expect("a digest is text");
    assert_ne!(
        stored,
        retry.payload_digest(),
        "a new thread differs from the stored digest, which is what logs the drift"
    );

    fixtures.cleanup().await;
}

/// The lookup index slot 918 adds is partial on the destination itself.
///
/// Asserted against `pg_indexes`, as the delivery-lookup suite does, so what is
/// graded is the index the database built. The predicate is the point: only
/// rows that owe an answer enter it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_reply_lookup_index_holds_only_rows_that_owe() {
    let fixtures = Fixtures::create_with_queue().await;
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let definition: String = sqlx::query("SELECT indexdef FROM pg_indexes WHERE indexname = $1")
        .bind(LOOKUP_INDEX)
        .fetch_one(&mut *connection)
        .await
        .unwrap_or_else(|failure| panic!("{LOOKUP_INDEX} must exist after migration: {failure}"))
        .try_get(0)
        .expect("an index definition is text");
    assert!(
        definition.contains(LOOKUP_COLUMNS),
        "the lookup keys on the fleet and the logical id's two integers: {definition}"
    );
    assert!(
        definition.contains(LOOKUP_PREDICATE),
        "only rows with a destination enter the index: {definition}"
    );

    fixtures.cleanup().await;
}
