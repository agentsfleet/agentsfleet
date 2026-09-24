//! One admitted event driven to settlement by the real verbs, and the
//! assertion that a recovery found it again.
//!
//! Split from the crash-boundary cases at the file cap; both cases drive
//! events through these.

use super::*;

/// Asserts every event in `owed` is recoverable again: its row names a receipt,
/// the stream holds that receipt's entry, and it was re-appended `replays` times.
///
/// `copies` of each logical id are expected on the stream — one after a
/// recovery, two where the entry a dead inserter left is still there beside the
/// replay's.
pub(super) async fn assert_recovered(
    fixtures: &Fixtures,
    streams: &FleetStreams,
    fleet: &str,
    owed: &[String],
    entries: &[(String, String)],
    copies: usize,
) {
    for recovered in owed {
        let receipt = fixtures
            .admission_receipt(fleet, recovered)
            .await
            .unwrap_or_else(|| panic!("{recovered} is accepted work and must hold a receipt"));
        assert_eq!(
            fixtures.admission_replays(fleet, recovered).await,
            1,
            "{recovered} was re-appended exactly once"
        );
        assert!(
            streams
                .holds_entry(fleet, &EventId::of(&receipt))
                .await
                .expect("the stream answers"),
            "{recovered}'s receipt must name an entry the stream holds"
        );
        assert_eq!(
            entries
                .iter()
                .filter(|(_receipt, event_id)| event_id == recovered)
                .count(),
            copies,
            "{recovered} must sit on {copies} physical entries: {entries:?}"
        );
    }
}

/// One admitted event, driven through delivery and settlement by the real
/// verbs, and the three facts a recovery assertion needs afterwards.
///
/// Every step here is production code: `select` takes the entry off the stream,
/// `record_received` opens the narrative log AND stamps `delivered_at`, and
/// `claim_and_settle` draws the wallet down. Nothing in this file writes the
/// delivery stamp itself, which is the whole reason this suite lives in
/// `afd_fleet` — a test that bound `MARK_DELIVERED` by hand would keep passing
/// the day the lease path stopped running it.
pub(super) struct Completed {
    /// The logical event the lease took.
    pub(super) event_id: String,
    /// The receipt it arrived on, which it must keep across the loss.
    pub(super) receipt: String,
    /// The tenant's balance after exactly one settled run.
    pub(super) balance: Option<i64>,
    /// How many ledger rows that run's charge wrote.
    pub(super) debits: i64,
}

pub(super) async fn run_one_to_settlement(
    fixtures: &Fixtures,
    leases: &afd_fleet::lease::Leases,
    fleet: &str,
    tenant: &str,
    runner: &Uuid7,
    now: UnixMillis,
) -> Completed {
    let settled_at = now.saturating_add_millis(SLICE_MS);
    // Scoped to THIS fleet, not "whatever any partition offers". The expect
    // below always claimed it reached the fleet holding admitted work while
    // accepting any leasable fleet, and every line after it reads `fleet` --
    // `admission_receipt(fleet, &event_id)` pairs the parameter with an
    // event_id taken from the acquisition. A lease won on a neighbour's fleet
    // pairs an id with a fleet that never held it, which is a wrong answer
    // rather than a flake. The readiness cursor is global, so this is decided
    // by what else is polling.
    let acquired = crate::seed::select_fleet_within_rotations(leases, runner, now, fleet)
        .await
        .expect("one rotation of polls must reach the fleet holding admitted work");
    let event_id = acquired.event_id.clone();
    assert_eq!(
        leases
            .record_received(&acquired, now)
            .await
            .expect("the narrative log must open")
            .delivery,
        Delivery::First,
        "a newly leased event has no row yet"
    );
    let receipt = fixtures
        .admission_receipt(fleet, &event_id)
        .await
        .expect("the delivered row still names the entry it arrived on");
    assert!(
        fixtures
            .admission_delivered_at(fleet, &event_id)
            .await
            .is_some(),
        "the REAL delivery path stamped the ledger — nothing in this test did"
    );

    let tenant_id = Uuid7::parse(tenant).expect("the fixture id is a v7 spelling");
    let issued = leases
        .issue(
            runner,
            &acquired,
            afd_fleet::lease::Billed {
                tenant_id: &tenant_id,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            now,
        )
        .await
        .expect("the lease row must be written");
    let Settled::Claimed(charged) = fixtures
        .settle_alone(
            leases,
            issued.lease_id.as_str(),
            runner,
            run_fee_meter(),
            true,
            settled_at,
        )
        .await
    else {
        unreachable!("the only holder of this fleet cannot be fenced out")
    };
    assert_eq!(
        charged.as_i64(),
        SLICE_NANOS,
        "a second of runtime at one nano per millisecond is a thousand nanos"
    );
    let balance = fixtures.balance(tenant).await;
    assert_eq!(
        balance,
        Some(DEEP_POOL - SLICE_NANOS),
        "the wallet paid for the run exactly once"
    );
    let debits = fixtures.ledger_rows(&event_id).await;

    Completed {
        event_id,
        receipt,
        balance,
        debits,
    }
}

/// Appends the entry a crashed inserter had already put on the stream.
///
/// The same fields `admit` writes, under the same logical id, so what the
/// stream holds afterwards is indistinguishable from an append whose caller
/// died before recording its receipt.
pub(super) async fn append_as_the_dead_inserter(
    streams: &FleetStreams,
    fleet: &str,
    workspace: &str,
    event_id: &str,
    now: UnixMillis,
) -> EventId {
    let created_at = now.as_millis().to_string();
    let entry = Entry {
        actor: ACTOR,
        event_type: EventType::Webhook.as_str(),
        workspace_id: workspace,
        request_json: REQUEST_JSON,
        created_at: &created_at,
    };
    streams
        .append(fleet, &entry.queued_pairs(event_id))
        .await
        .expect("the lane's stream takes an append")
}
