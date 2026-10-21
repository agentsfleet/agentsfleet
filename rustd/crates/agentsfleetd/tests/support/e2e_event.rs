//! Production admissions and an unsupported event fixture, both stored in the ledger.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Admission, Admissions, Key, Producer};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_wire::event::EventType;
use agentsfleetd::serve::Booted;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use crate::e2e::{ACTOR, REQUEST_JSON};

/// The digest a hand-written row carries. Never compared: the ledger checks
/// it only on a REPEATED key, and this row's key is its own id.
const UNCHECKED_DIGEST: &str = "fixture";

/// Admits one event on the fleet and answers the ledger's logical id.
///
/// The type is an [`EventType`] rather than a string because the ledger takes
/// one: the set is CLOSED, and a scenario that wants a spelling the daemon
/// cannot name is testing a different thing — see [`enqueue_unsupported`].
pub(crate) async fn enqueue(
    booted: &Booted,
    fleet: &str,
    workspace: &str,
    event_type: EventType,
) -> String {
    let admitted = Admissions::for_tests(booted.database.clone(), booted.queue.clone())
        .admit(Admission {
            producer: Producer::Steer,
            key: Key::Unrepeatable,
            fleet,
            workspace,
            actor: ACTOR,
            event_type,
            request_json: REQUEST_JSON,
            reply: afd_admission::Reply::None,
        })
        .await
        .expect("the ledger must admit the event");
    assert!(
        !admitted.replayed,
        "an unrepeatable key is minted per call and cannot replay"
    );
    admitted.stored.id
}

/// Bypasses the closed producer enum to exercise refusal of an unknown event type.
pub(crate) async fn enqueue_unsupported(
    booted: &Booted,
    fleet: &str,
    workspace: &str,
    event_type: &str,
    now: Timestamp,
) -> String {
    insert_admission(booted, fleet, workspace, event_type, now)
        .await
        .as_str()
        .to_owned()
}

/// The ledger row for [`enqueue_unsupported`]: its id, which is the event's.
async fn insert_admission(
    booted: &Booted,
    fleet: &str,
    workspace: &str,
    event_type: &str,
    now: Timestamp,
) -> Uuid7 {
    let row_id = Uuid7::encode(
        now,
        Entropy::new()
            .uuid_randomness()
            .expect("the host's entropy must answer"),
    )
    .expect("a version-7 id");
    sqlx::query(
        "INSERT INTO fleet.admissions
           (id, fleet_id, workspace_id, producer, producer_key, payload_digest,
            actor, event_type, request_json, event_created_at,
            created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $1, $5, $6, $7, $8, $9, $9, $9)",
    )
    .bind(row_id.as_str())
    .bind(fleet)
    .bind(workspace)
    .bind(Producer::Steer.as_str())
    .bind(UNCHECKED_DIGEST)
    .bind(ACTOR)
    .bind(event_type)
    .bind(REQUEST_JSON)
    .bind(now.to_sqlx())
    .execute(
        &mut *booted
            .database
            .acquire()
            .await
            .expect("a pooled connection"),
    )
    .await
    .expect("the admission row must insert");
    row_id
}
