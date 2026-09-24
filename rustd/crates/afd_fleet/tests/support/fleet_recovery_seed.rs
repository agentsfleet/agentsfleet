//! Staging a fleet whose work a dead runner is still holding.
//!
//! The state the readiness index cannot describe and no stream will surface:
//! an admission that was delivered, leased, and then abandoned when its holder
//! stopped answering. Ingress marked the fleet as it admitted, the mark was
//! spent by the poll that leased it, and nothing re-marks it — so the ledger
//! is the only thing that still knows work is owed.
//!
//! The dead holder costs nothing to stage. Every lease issued at the fixtures'
//! instant is months behind the real clock, so a lease left unsettled is
//! `active` and past `lease_expires_at` by the only clock the sweeper reads.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Admission, Admissions, Key, Producer};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_dragonfly::FleetStreams;
use afd_fleet::lease::{Billed, Delivery, Leases};
use afd_wire::event::EventType;

use crate::report_seed::DEEP_POOL;
use crate::requests::ENROLLED_AT;
use crate::seed::{MODEL, POSTURE, PROVIDER, seeded_parts, select_fleet_within_rotations};
use crate::support::Fixtures;

const ACTOR: &str = "webhook:recovery";
const REQUEST_JSON: &str = r#"{"delivery":"recovery"}"#;

/// One fleet staged mid-flight, and the handles a proof needs back.
pub(crate) struct Abandoned {
    pub(crate) fleet: String,
    /// A second runner, which polls after the sweeper has run.
    pub(crate) poller: Uuid7,
    pub(crate) event_id: String,
    /// The lease the holder that never came back is still on.
    pub(crate) lease_id: String,
}

/// An admission for `fleet`, keyed so no two fleets dedup onto one row:
/// `(producer, producer_key)` is unique across the whole ledger.
pub(crate) fn admission<'a>(fleet: &'a str, workspace: &'a str, key: &'a str) -> Admission<'a> {
    Admission {
        producer: Producer::Webhook,
        key: Key::Repeated(key),
        fleet,
        workspace,
        actor: ACTOR,
        event_type: EventType::Webhook,
        request_json: REQUEST_JSON,
        reply: afd_admission::Reply::None,
    }
}

/// The ledger over this fixture's datastores.
pub(crate) fn ledger(fixtures: &Fixtures) -> Admissions {
    Admissions::for_tests(fixtures.database.clone(), fixtures.queue().clone())
}

/// A fleet holding one delivered, leased, never-settled admission, with its
/// stream and readiness mark thrown away.
///
/// The flush is what makes this the ledger's problem alone: with the stream
/// gone, the reclaim sweeper's stream probe can vouch for nothing, so a mark
/// can only come from the question it asks Postgres.
pub(crate) async fn abandoned_mid_flight(fixtures: &Fixtures, leases: &Leases) -> Abandoned {
    let staged_at = UnixMillis::from_millis(ENROLLED_AT);
    let (fleet, workspace, tenant, [holder, poller]) = seeded_parts::<2>(fixtures).await;
    fixtures.seed_wallet(&tenant, DEEP_POOL, ENROLLED_AT).await;
    let key = format!("{fleet}:abandoned");
    let admitted = ledger(fixtures)
        .admit(admission(&fleet, &workspace, &key))
        .await
        .expect("a live queue admits and receipts");

    // Narrowed to THIS fleet. The readiness partition cursor is process-global
    // and every suite in this binary turns it, so "one rotation" is one
    // rotation only when nothing else is polling — and the assertion below
    // pairs the acquired event with this fleet's admission, which a poll that
    // answered a sibling's work would fail for the wrong reason.
    let acquired = select_fleet_within_rotations(leases, &holder, staged_at, &fleet)
        .await
        .expect("the fleet holding admitted work is offered within the rotations polled");
    assert_eq!(acquired.event_id, admitted.id);
    assert_eq!(
        leases
            .record_received(&acquired, staged_at)
            .await
            .expect("the narrative log opens")
            .delivery,
        Delivery::First
    );
    let tenant_id = Uuid7::parse(&tenant).expect("a v7 fixture id");
    let issued = leases
        .issue(
            &holder,
            &acquired,
            Billed {
                tenant_id: &tenant_id,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            staged_at,
        )
        .await
        .expect("the lease row is written");

    FleetStreams::new(fixtures.queue().clone())
        .forget(&fleet)
        .await
        .expect("destroying the fleet's stream data");
    crate::queue::clear_ready(fixtures.queue(), &fleet).await;

    Abandoned {
        fleet,
        poller,
        event_id: admitted.id,
        lease_id: issued.lease_id.as_str().to_owned(),
    }
}
