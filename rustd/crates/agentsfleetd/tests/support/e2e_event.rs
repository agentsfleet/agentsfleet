//! The work a §7 scenario hands its runner, admitted the way production
//! admits it.
//!
//! Split from `e2e.rs` by concern rather than by size (RULE FLL): that file
//! owns the LIFECYCLE — boot a daemon, seed its rows, tear it down — and this
//! one owns the EVENT: what goes on the fleet's stream, and through which verb.
//!
//! # Through the ledger, never a raw append
//!
//! The pull path refuses a stream entry that carries no `event_id`
//! (`afd_fleet::lease::envelope::from_fresh`): the entry id is a receipt, and
//! the field is the admission ledger's logical id, so an entry without it was
//! appended by something that never admitted it. This seed once did exactly
//! that — a bare `XADD` of the pre-ledger field set — and the outcome was the
//! one that module's documentation predicts: every entry durable, delivered,
//! and undecodable. The poll that finally reached the fleet dropped the entry
//! as `assign_entry_undecodable_dropped`, and every poll after it answered
//! `no leasable work`, which is a documented answer and read for a whole
//! session as a readiness peek that disagreed with itself. So the seed calls
//! [`Admissions::admit`], the verb every producer calls, and takes its
//! `event_id`, its entry and its readiness mark from the code the daemon ships.
//!
//! The consumer group is the one thing a scenario still creates by hand:
//! production makes it at fleet install, and a scenario seeds its fleet row
//! directly rather than installing one.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Admission, Admissions, Key, Producer, logical_id};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::{FleetStreams, ReadyIndex};
use afd_wire::event::{Entry, EventType};
use agentsfleetd::serve::Booted;
use sqlx::Row as _;

use crate::e2e::{ACTOR, REQUEST_JSON};

/// A row's `replay_count` on the day it is admitted.
const NO_REPLAYS: i64 = 0;

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
    ensure_group(booted, fleet).await;
    let admitted = Admissions::for_tests(booted.database.clone(), booted.queue.clone())
        .admit(Admission {
            producer: Producer::Steer,
            key: Key::Unrepeatable,
            fleet,
            workspace,
            actor: ACTOR,
            event_type,
            request_json: REQUEST_JSON,
        })
        .await
        .expect("the ledger must admit the event");
    assert!(
        !admitted.replayed,
        "an unrepeatable key is minted per call and cannot replay"
    );
    admitted.id
}

/// An event whose type this daemon cannot name — what a producer from a newer
/// build writes.
///
/// [`Admission`] takes an [`EventType`], so the ledger cannot be asked to admit
/// a spelling it does not know. The row and the entry are written here the way
/// the ledger writes them — logical id from the row, six fields on the entry,
/// receipt recorded, fleet marked — with a type `EventType::parse` refuses.
/// Which is the point: the daemon must END that delivery rather than retry it
/// forever, and a fixture that skipped the row would be proving that against
/// a shape no producer makes.
pub(crate) async fn enqueue_unsupported(
    booted: &Booted,
    fleet: &str,
    workspace: &str,
    event_type: &str,
    now: UnixMillis,
) -> String {
    ensure_group(booted, fleet).await;
    let (row_id, event_id) = insert_admission(booted, fleet, workspace, event_type, now).await;
    let created_at = now.as_millis().to_string();
    let entry = Entry {
        actor: ACTOR,
        event_type,
        workspace_id: workspace,
        request_json: REQUEST_JSON,
        created_at: &created_at,
    };
    let receipt = FleetStreams::new(booted.queue.clone())
        .append(fleet, &entry.queued_pairs(&event_id))
        .await
        .expect("the entry must append");
    sqlx::query(
        "UPDATE core.fleet_admissions SET receipt = $2, updated_at = $3 WHERE id = $1::uuid",
    )
    .bind(row_id.as_str())
    .bind(receipt.as_str())
    .bind(now.as_millis())
    .execute(
        &mut *booted
            .database
            .acquire()
            .await
            .expect("a pooled connection"),
    )
    .await
    .expect("the receipt must record");
    ReadyIndex::new(booted.queue.clone())
        .mark(fleet, fleet)
        .await
        .expect("the readiness mark must land");
    event_id
}

/// The ledger row for [`enqueue_unsupported`]: its id, and the logical id the
/// row's own `created_at` and `seq` spell.
async fn insert_admission(
    booted: &Booted,
    fleet: &str,
    workspace: &str,
    event_type: &str,
    now: UnixMillis,
) -> (Uuid7, String) {
    let row_id = Uuid7::encode(
        now,
        Entropy::new()
            .uuid_randomness()
            .expect("the host's entropy must answer"),
    )
    .expect("a version-7 id");
    let row = sqlx::query(
        "INSERT INTO core.fleet_admissions
           (id, fleet_id, workspace_id, producer, producer_key, payload_digest,
            actor, event_type, request_json, event_created_at, replay_count,
            created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $1, $5, $6, $7, $8, $9, $10, $9, $9)
         RETURNING created_at, seq",
    )
    .bind(row_id.as_str())
    .bind(fleet)
    .bind(workspace)
    .bind(Producer::Steer.as_str())
    .bind(UNCHECKED_DIGEST)
    .bind(ACTOR)
    .bind(event_type)
    .bind(REQUEST_JSON)
    .bind(now.as_millis())
    .bind(NO_REPLAYS)
    .fetch_one(
        &mut *booted
            .database
            .acquire()
            .await
            .expect("a pooled connection"),
    )
    .await
    .expect("the admission row must insert");
    let created_at: i64 = row.try_get(0).expect("created_at is a bigint");
    let seq: i64 = row.try_get(1).expect("seq is a bigint");
    (row_id, logical_id(created_at, seq))
}

/// The consumer group the daemon reads the fleet under.
async fn ensure_group(booted: &Booted, fleet: &str) {
    FleetStreams::new(booted.queue.clone())
        .ensure_group(fleet)
        .await
        .expect("the consumer group must exist before a read");
}
