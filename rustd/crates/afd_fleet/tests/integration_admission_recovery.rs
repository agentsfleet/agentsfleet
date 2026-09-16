//! Dimensions 2.1 and 2.2 — what the shipped admission ledger recovers, and
//! what recovery must never charge for twice.
//!
//! The prototype suite proved the SHAPE against a `TEMP TABLE` and a raw
//! driver, before
//! `core.fleet_admissions` existed. This proves the real thing, and it lives in
//! `afd_fleet` rather than beside the ledger because the claims are about both
//! ends at once: `Admissions::admit` and its two recovery passes on one side,
//! and `Leases::record_received` and `claim_and_settle` on the other. Only this
//! crate reaches all four, so only here can "one settlement and one debit" be
//! DRIVEN rather than modelled. A test that bound the delivery stamp itself
//! would keep passing the day the lease path stopped writing it.
//!
//! # The three crash boundaries, and how each is reached
//!
//! A crash between commit and append is not simulated — it is PRODUCED, by
//! handing the ledger a queue nothing listens on, so `admit` takes its real
//! deferral arm: the row commits and the receipt stays NULL. A crash between
//! append and receipt is produced by appending the entry the way the dead
//! inserter had. A crash after the receipt is a healthy admission, and what it
//! proves is that recovery leaves it alone.
//!
//! # Why no pass count is ever asserted
//!
//! `SELECT_UNRECEIPTED` and `SELECT_UNDELIVERED_FLEETS` carry no fleet
//! predicate — both sweepers are deployment-wide by design — so a sibling
//! suite's deferred row is scanned, and sometimes repaired, by a pass this file
//! starts. `Replayed::scanned` and `Reconciled::voided` are therefore other
//! tests' business as much as this one's. Every assertion below reads this
//! test's own minted fleet back out of the ledger, the stream, or the wallet
//! (ISO-1); none reads a counter.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly, naming \
              which of several rows it was"
)]

use std::slice;
use std::time::Duration;

use afd_admission::{Admission, Admissions, Key, Producer};
use afd_core::clock::{self, UnixMillis};
use afd_core::id::Uuid7;
use afd_dragonfly::{EventId, FleetStreams};
use afd_fleet::lease::{Delivery, Settled};
use afd_wire::event::{Entry, EventType};
use tokio::sync::Mutex;

use crate::queue;
use crate::report_seed::{DEEP_POOL, SLICE_MS, SLICE_NANOS, run_fee_meter};
use crate::requests::ENROLLED_AT;
use crate::seed::{MODEL, POSTURE, PROVIDER, seeded_parts};
use crate::support::Fixtures;

/// How many fleets one reconcile pass here may examine.
///
/// Generous, because the scan is deployment-wide: a budget of one would spend
/// itself on whichever fleet a sibling suite minted and never reach this one.
/// Serialises the two tests here, because BOTH run global recovery sweeps.
///
/// `reconcile` and `replay` take batch LIMITS, not a fleet filter — `fleets`
/// caps how many fleets a pass examines, it does not choose which. So a pass
/// one test runs reaches the fleet the other test seeded, and the second replay
/// lands on a row that asserts it was re-appended exactly once. Observed in
/// Continuous Integration as `admission_replays == 2`; it passes locally
/// whenever the two happen not to overlap, which is what makes it worth a lock
/// rather than a retry.
///
/// A `static` serialises within ONE process, which is enough only because
/// `afd_fleet` sets `autotests = false` and every `tests/*.rs` here compiles
/// into a single binary. Splitting this suite would silently unguard it.
///
/// The shape is `afd_dragonfly`'s `HUB_LANE`, for the same reason: a lock with
/// no payload, because what it protects is the datastore's global row set and
/// not anything held inside it. `tokio`'s rather than `std`'s because the guard
/// is held across `.await`.
pub(crate) static RECOVERY_LANE: Mutex<()> = Mutex::const_new(());

pub(crate) const EVERY_FLEET: i64 = 4_096;

/// How many rows a pass repairs or re-appends. Larger than anything admitted
/// here.
pub(crate) const EVERY_ROW: i64 = 256;

/// The grace a replay pass gives an in-flight admission to receipt itself.
///
/// Zero: the rows deferred below were deferred a millisecond ago, and the
/// production cutoff would skip every one of them.
pub(crate) const NO_GRACE: Duration = Duration::ZERO;

/// The actor and body every admission here carries.
const ACTOR: &str = "webhook:recovery";
const REQUEST_JSON: &str = r#"{"delivery":"recovery"}"#;

/// A producer key nothing else in the deployment can collide with.
///
/// `uq_fleet_admissions_producer_key` is UNIQUE on `(producer, producer_key)`
/// across the whole table — NOT per fleet, because a webhook delivery id is
/// globally unique and deduplicating it per fleet would let one delivery run
/// twice. So a fixed key here is not merely untidy: the second run of this
/// suite would conflict with the first, be answered the FIRST run's event id,
/// and then look for that id under this run's fleet (ISO-1).
///
/// Minted off the fleet, which `seeded_parts` already minted.
fn producer_key(fleet: &str, delivery: &str) -> String {
    format!("{fleet}:{delivery}")
}

/// One webhook admission for `fleet`, keyed by `delivery`.
fn admission<'a>(fleet: &'a str, workspace: &'a str, delivery: &'a str) -> Admission<'a> {
    Admission {
        producer: Producer::Webhook,
        key: Key::Repeated(delivery),
        fleet,
        workspace,
        actor: ACTOR,
        event_type: EventType::Webhook,
        request_json: REQUEST_JSON,
    }
}

/// A ledger over the lane's database and the lane's real queue.
fn ledger(fixtures: &Fixtures) -> Admissions {
    Admissions::for_tests(fixtures.database.clone(), fixtures.queue().clone())
}

/// A ledger over the lane's database and a queue that is not there.
///
/// `queue::unreachable` is the lane's own helper and says why it is a private
/// handle rather than a paused container: the lane's datastore is shared by
/// every binary running in parallel, so a handle one test owns is the only way
/// to fail one test's commands.
fn deferring(fixtures: &Fixtures) -> Admissions {
    Admissions::for_tests(fixtures.database.clone(), queue::unreachable())
}

/// Dimension 2.1 — a stop injected between commit, append and receipt leaves
/// zero missing admitted events after replay.
///
/// The three boundaries in one test because they are three points on ONE
/// timeline, and the interesting assertion at each is what the others did not
/// do: the healthy row must not be re-appended, and the row appended twice
/// must still be one logical event.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn acceptance_recovers_at_each_crash_boundary() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<1>(&fixtures).await;
    let streams = FleetStreams::new(fixtures.queue().clone());
    let live = ledger(&fixtures);
    let now = UnixMillis::from_millis(ENROLLED_AT);

    // ── Stop after the receipt: a healthy admission, so the pass below has
    // something it must leave alone.
    let healthy_key = producer_key(&fleet, "healthy");
    let healthy = live
        .admit(admission(&fleet, &workspace, &healthy_key))
        .await
        .expect("a live queue admits and receipts in one call");
    let healthy_receipt = fixtures
        .admission_receipt(&fleet, &healthy.id)
        .await
        .expect("a live append records its receipt");

    // ── Stop between commit and append: the queue is not there, so `admit`
    // commits the row and defers the append for real.
    let deferred_key = producer_key(&fleet, "deferred");
    let deferred = deferring(&fixtures)
        .admit(admission(&fleet, &workspace, &deferred_key))
        .await
        .expect("the row commits whatever the queue does");
    assert!(
        !deferred.replayed,
        "a first admission is not a replay of itself"
    );
    assert_eq!(
        fixtures.admission_receipt(&fleet, &deferred.id).await,
        None,
        "an append that never happened records no receipt"
    );

    // ── Stop between append and receipt: the same row, plus the entry the dead
    // inserter had already put on the stream.
    let half_key = producer_key(&fleet, "half-appended");
    let half = deferring(&fixtures)
        .admit(admission(&fleet, &workspace, &half_key))
        .await
        .expect("the row commits whatever the queue does");
    let orphan = append_as_the_dead_inserter(&streams, &fleet, &workspace, &half.id, now).await;

    // The REAL clock, not the fixture's `now`. `replay`'s cutoff is
    // `now - min_age` against `core.fleet_admissions.created_at`, which `admit`
    // stamps with `clock::now()` — so a pass given the fixture's fixed instant
    // finds every row "newer" than its cutoff and scans nothing. The fixture
    // instant governs lease and billing arithmetic; the ledger's own rows are
    // wall-clock, and the two must not be crossed.
    live.replay(clock::now(), NO_GRACE, EVERY_ROW)
        .await
        .expect("the replay pass runs against both live datastores");

    // Zero missing: every admitted row names an entry the stream holds. The row
    // whose append had already landed carries TWO physical copies of one
    // logical id — the dead inserter's entry and the replay's — which the
    // architecture page states outright and the delivery path's conflict arm is
    // what resolves.
    let entries = queue::entries_on(fixtures.queue(), &fleet).await;
    assert_recovered(
        &fixtures,
        &streams,
        &fleet,
        slice::from_ref(&deferred.id),
        &entries,
        1,
    )
    .await;
    assert_recovered(
        &fixtures,
        &streams,
        &fleet,
        slice::from_ref(&half.id),
        &entries,
        2,
    )
    .await;

    // The healthy row kept its receipt and was never replayed.
    assert_eq!(
        fixtures.admission_receipt(&fleet, &healthy.id).await,
        Some(healthy_receipt),
        "a receipted admission keeps the receipt it had"
    );
    assert_eq!(
        fixtures.admission_replays(&fleet, &healthy.id).await,
        0,
        "a receipted admission is invisible to the replay scan"
    );

    // The ledger still holds one row per producer key, and the entry the dead
    // inserter appended is one of the two the logical id now sits on.
    assert!(
        entries
            .iter()
            .any(|(receipt, _event_id)| receipt == orphan.as_str()),
        "the entry the dead inserter appended is still there: {entries:?}"
    );
    assert_eq!(
        fixtures.admissions_for(&fleet).await,
        3,
        "three producer keys are three ledger rows, whatever the stream holds"
    );

    streams.forget(&fleet).await.expect("purging the stream");
    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}

/// Dimension 2.2 — destroying the queue's data and rebuilding it replays every
/// unfinished admission, with one settlement and one debit.
///
/// The run that already settled is the subject. Its entry is destroyed along
/// with everyone else's, and the only thing distinguishing it from accepted
/// work that must be re-appended is `delivered_at` — stamped by the real
/// `record_received`, not by this test. Without that column recovery re-queues
/// a completed run, and the wallet pays for it twice.
///
/// `forget` is the loss: `DEL` on the stream key takes the entries and the
/// consumer group with them, scoped to this test's own minted fleet, which is
/// the isolated equivalent of a flush and the only one permitted against a
/// shared datastore.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn queue_loss_replays_without_duplicate_settlement() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let streams = FleetStreams::new(fixtures.queue().clone());
    let leases = fixtures.leases();
    let live = ledger(&fixtures);
    let now = UnixMillis::from_millis(ENROLLED_AT);
    fixtures.seed_wallet(&tenant, DEEP_POOL, ENROLLED_AT).await;

    // Three admissions: the lease will take one and run it to settlement, and
    // the other two are the accepted work the loss must not lose.
    let mut owed = Vec::new();
    for delivery in ["lost-one", "lost-two", "lost-three"] {
        let key = producer_key(&fleet, delivery);
        let event = live
            .admit(admission(&fleet, &workspace, &key))
            .await
            .expect("a live queue admits and receipts");
        assert!(
            fixtures
                .admission_receipt(&fleet, &event.id)
                .await
                .is_some(),
            "{key} was receipted before the loss"
        );
        owed.push(event.id);
    }

    // ── The run that completes, driven through the real verbs. Extracted
    // because it is not about recovery: it is admission and settlement running
    // to completion,
    // which is the precondition the loss below is injected into.
    let completed = run_one_to_settlement(&fixtures, &leases, &fleet, &tenant, &runner, now).await;
    let before = owed.len();
    owed.retain(|pending| pending != &completed.event_id);
    assert_eq!(
        owed.len(),
        before - 1,
        "the lease took one of the admitted events, and it is no longer owed"
    );
    assert_eq!(owed.len(), 2, "two admissions are still owed to the queue");

    // ── The loss. Entries and consumer group, gone.
    streams
        .forget(&fleet)
        .await
        .expect("destroying this fleet's stream data");
    assert!(
        queue::entries_on(fixtures.queue(), &fleet).await.is_empty(),
        "the stream holds nothing after the loss"
    );

    // ── The rebuild. Reconcile forgets the receipts whose entries are gone;
    // replay re-appends the rows that are owed again.
    // Both passes on the REAL clock, for the reason the 2.1 test states: the
    // ledger stamps `created_at` with `clock::now()`, so a replay cutoff built
    // from the fixture's instant scans nothing.
    let repairing_at = clock::now();
    live.reconcile(repairing_at, EVERY_FLEET, EVERY_ROW)
        .await
        .expect("the reconcile pass runs against both live datastores");
    live.replay(repairing_at, NO_GRACE, EVERY_ROW)
        .await
        .expect("the replay pass runs against both live datastores");

    // Zero accepted work missing: each owed row names a live entry again, once.
    let entries = queue::entries_on(fixtures.queue(), &fleet).await;
    assert_recovered(&fixtures, &streams, &fleet, &owed, &entries, 1).await;

    // The run that already settled was not re-queued. This is the assertion
    // `delivered_at` exists for.
    assert_eq!(
        fixtures
            .admission_receipt(&fleet, &completed.event_id)
            .await,
        Some(completed.receipt.clone()),
        "a delivered admission keeps the receipt it ran under, gone entry or not"
    );
    assert_eq!(
        fixtures
            .admission_replays(&fleet, &completed.event_id)
            .await,
        0,
        "a delivered admission is never re-appended"
    );
    assert!(
        !entries
            .iter()
            .any(|(_receipt, event_id)| event_id == &completed.event_id),
        "nothing re-queued the run that already completed: {entries:?}"
    );

    // One settlement and one debit: the wallet did not move across the loss and
    // the rebuild, and the run's ledger rows did not multiply.
    assert_eq!(
        fixtures.balance(&tenant).await,
        completed.balance,
        "recovery charged the tenant nothing: the only run that ran was settled before the loss"
    );
    assert_eq!(
        fixtures.ledger_rows(&completed.event_id).await,
        completed.debits,
        "the completed run's debits did not multiply across the rebuild"
    );

    // The ledger never grew: recovery re-appends entries, never identities.
    assert_eq!(
        fixtures.admissions_for(&fleet).await,
        3,
        "three producer keys are still three ledger rows after a full rebuild"
    );

    streams.forget(&fleet).await.expect("purging the stream");
    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}

/// Asserts every event in `owed` is recoverable again: its row names a receipt,
/// the stream holds that receipt's entry, and it was re-appended `replays` times.
///
/// `copies` of each logical id are expected on the stream — one after a
/// recovery, two where the entry a dead inserter left is still there beside the
/// replay's.
async fn assert_recovered(
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
struct Completed {
    /// The logical event the lease took.
    event_id: String,
    /// The receipt it arrived on, which it must keep across the loss.
    receipt: String,
    /// The tenant's balance after exactly one settled run.
    balance: Option<i64>,
    /// How many ledger rows that run's charge wrote.
    debits: i64,
}

async fn run_one_to_settlement(
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
async fn append_as_the_dead_inserter(
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
