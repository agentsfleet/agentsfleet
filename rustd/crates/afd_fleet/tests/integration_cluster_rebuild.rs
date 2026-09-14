//! Dimension 7.8 — Dragonfly is a cache and PostgreSQL the forge: throw the
//! cache away, refill it from the ledger, and nothing accepted is lost.
//!
//! # What is thrown away
//!
//! Per fleet, everything this test put in the datastore: the stream — entries
//! and consumer group, via `forget` — and the readiness mark. That is the
//! isolated equivalent of a flush and the only one the shared rig permits, so
//! it is applied to every fleet this test minted rather than to the cluster.
//!
//! # What is seeded, and why each state is its own fleet
//!
//! One fleet per state, so the flush and every assertion are per fleet and a
//! state cannot mask another: admitted and never receipted; receipted and
//! never delivered; delivered under a lease its holder is still renewing; and
//! delivered under a lease whose holder died — `BATCH_LIMIT + 1` of that last
//! one, because it is the state nothing recovered before the ledger question
//! and the one past the page is what proves the question's cursor wraps.
//!
//! The dead holder costs nothing to stage. Every lease here is issued at the
//! fixtures' instant, which is months behind the real clock, so a lease left
//! unsettled is `active` and past `lease_expires_at` by the only clock the
//! sweeper reads. The live holder is the one that has to be issued at the real
//! instant.
//!
//! # What "recovered" means
//!
//! Not that a sweeper visited the row: that the ordinary worker path obtains
//! the work. So after the rebuild each stranded fleet is POLLED, through the
//! same rotation a runner uses, and the poll must hand back the event the
//! dead holder never finished, under a fresh fence, with the dead lease
//! expired on the way — `reclaim_prior_active` doing the one job it exists
//! for, from PostgreSQL alone, on a stream that holds nothing.
//!
//! # Not covered here, named rather than implied
//!
//! An outbound answer owed (its own harness, `afd_outbound`), and the
//! `session` store, whose loss is a decision the spec records rather than a
//! rebuild this test could assert.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_admission::{Admission, Admissions, Key, Producer};
use afd_core::clock::{self, UnixMillis};
use afd_core::id::Uuid7;
use afd_datastore::FleetStreams;
use afd_fleet::lease::{Billed, Delivery, Leases, runner_consumer};
use afd_runner::sweep::rebuild::rebuild;
use afd_runner::sweep::reclaim::{BATCH_LIMIT, Reclaim};
use afd_runner::sweep::reconcile::Reconcile;
use afd_runner::sweep::replay::Replay;
use afd_wire::event::EventType;

use crate::queue;
use crate::report_seed::DEEP_POOL;
use crate::requests::ENROLLED_AT;
use crate::seed::{MODEL, POSTURE, PROVIDER, seeded_parts, select_within_one_rotation};
use crate::support::Fixtures;

/// How many stranded fleets: one past the page, so the second round of the
/// ledger question has to have advanced its cursor to reach the last one.
///
/// The sweeper's own integer type, converted where it is spent, so no cast
/// has to vouch for a bound the sweeper already asserts is positive.
const STRANDED: i64 = BATCH_LIMIT + 1;

/// Rounds a rebuild needs to mark every stranded fleet once: the pages the
/// envelope spans. Derived, so the envelope and the rounds cannot disagree.
// Ceiling division by hand: the signed `div_ceil` is not yet a const fn, and
// both operands are positive bounds.
const REBUILD_ROUNDS: i64 = (STRANDED + BATCH_LIMIT - 1) / BATCH_LIMIT;

const ACTOR: &str = "webhook:rebuild";
const REQUEST_JSON: &str = r#"{"delivery":"rebuild"}"#;
const DELIVERY: &str = "the-one-owed";

/// One fleet with work in one ledger state, and what the test needs back.
struct Staged {
    fleet: String,
    /// The runner that will poll after the rebuild.
    poller: Uuid7,
    event_id: String,
    /// The lease a dead or live holder left, where one was issued.
    lease_id: Option<String>,
}

fn admission<'a>(fleet: &'a str, workspace: &'a str) -> Admission<'a> {
    Admission {
        producer: Producer::Webhook,
        key: Key::Repeated(DELIVERY),
        fleet,
        workspace,
        actor: ACTOR,
        event_type: EventType::Webhook,
        request_json: REQUEST_JSON,
    }
}

fn ledger(fixtures: &Fixtures) -> Admissions {
    Admissions::for_tests(fixtures.database.clone(), fixtures.queue().clone())
}

/// A fleet whose one admission was delivered and leased at `at`, and never
/// settled. At the fixtures' instant that is a dead holder; at the real
/// instant, a live one.
async fn delivered_and_leased(fixtures: &Fixtures, leases: &Leases, at: UnixMillis) -> Staged {
    let (fleet, workspace, tenant, [holder, poller]) = seeded_parts::<2>(fixtures).await;
    fixtures.seed_wallet(&tenant, DEEP_POOL, ENROLLED_AT).await;
    let admitted = ledger(fixtures)
        .admit(admission(&fleet, &workspace))
        .await
        .expect("a live queue admits and receipts");

    let acquired = select_within_one_rotation(leases, &holder, at)
        .await
        .expect("one rotation of polls reaches the fleet holding admitted work");
    assert_eq!(acquired.event_id, admitted.id);
    assert_eq!(
        leases
            .record_received(&acquired, at)
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
            at,
        )
        .await
        .expect("the lease row is written");
    Staged {
        fleet,
        poller,
        event_id: admitted.id,
        lease_id: Some(issued.lease_id.as_str().to_owned()),
    }
}

/// A fleet whose one admission is receipted and sits on the stream unread.
async fn queued_undelivered(fixtures: &Fixtures) -> Staged {
    let (fleet, workspace, _tenant, [poller]) = seeded_parts::<1>(fixtures).await;
    let admitted = ledger(fixtures)
        .admit(admission(&fleet, &workspace))
        .await
        .expect("a live queue admits and receipts");
    Staged {
        fleet,
        poller,
        event_id: admitted.id,
        lease_id: None,
    }
}

/// A fleet whose one admission committed while the queue was away, so its
/// row has no receipt and nothing ever reached the stream.
async fn admitted_unreceipted(fixtures: &Fixtures) -> Staged {
    let (fleet, workspace, _tenant, [poller]) = seeded_parts::<1>(fixtures).await;
    let deferring = Admissions::for_tests(fixtures.database.clone(), queue::unreachable());
    let admitted = deferring
        .admit(admission(&fleet, &workspace))
        .await
        .expect("a queue that is away defers: the row commits and the caller is answered");
    assert!(
        fixtures
            .admission_receipt(&fleet, &admitted.id)
            .await
            .is_none(),
        "nothing receipted a row the queue never saw"
    );
    Staged {
        fleet,
        poller,
        event_id: admitted.id,
        lease_id: None,
    }
}

/// Throws away everything the datastore holds for one fleet.
async fn flush(fixtures: &Fixtures, streams: &FleetStreams, fleet: &str) {
    streams
        .forget(fleet)
        .await
        .expect("destroying the fleet's stream data");
    queue::clear_ready(fixtures.queue(), fleet).await;
    assert!(
        queue::entries_on(fixtures.queue(), fleet).await.is_empty(),
        "the stream holds nothing after the flush"
    );
}

/// One owed event is back on its fleet's stream, once, and its row names it.
async fn assert_reappended(fixtures: &Fixtures, staged: &Staged) {
    let entries = queue::entries_on(fixtures.queue(), &staged.fleet).await;
    assert_eq!(
        entries.len(),
        1,
        "fleet {} holds exactly its one owed event again: {entries:?}",
        staged.fleet
    );
    assert!(
        entries
            .iter()
            .any(|(_receipt, event_id)| event_id == &staged.event_id),
        "the re-appended entry is the logical event that was owed"
    );
    assert!(
        fixtures
            .admission_receipt(&staged.fleet, &staged.event_id)
            .await
            .is_some(),
        "the row names the entry it now rides"
    );
    assert_eq!(fixtures.admissions_for(&staged.fleet).await, 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_cluster_restart_and_stale_snapshot_preserve_obligations() {
    let fixtures = Fixtures::create_with_queue().await;
    let leases = fixtures.leases();
    let streams = FleetStreams::new(fixtures.queue().clone());
    let staged_at = UnixMillis::from_millis(ENROLLED_AT);

    // ── Every state the ledger can express, each on its own fleet.
    let population = usize::try_from(STRANDED).expect("a hundred-odd fleets fit a usize");
    let mut stranded = Vec::with_capacity(population);
    for _ in 0..population {
        stranded.push(delivered_and_leased(&fixtures, &leases, staged_at).await);
    }
    let live = delivered_and_leased(&fixtures, &leases, clock::now()).await;
    let queued = queued_undelivered(&fixtures).await;
    let unreceipted = admitted_unreceipted(&fixtures).await;
    let everyone = stranded
        .iter()
        .chain([&live, &queued, &unreceipted])
        .collect::<Vec<_>>();

    // ── The loss: every fleet's stream and mark, gone.
    for staged in &everyone {
        flush(&fixtures, &streams, &staged.fleet).await;
    }

    // ── The rebuild, through the real sweepers, in the order the module note
    // on `rebuild` explains: reconcile feeds replay, and reclaim marks last.
    let admissions = ledger(&fixtures);
    let reconcile = Reconcile::new(admissions.clone());
    let replay = Replay::new(admissions);
    let reclaim = Reclaim::new(
        fixtures.database.clone(),
        fixtures.queue().clone(),
        runner_consumer(),
    );
    let rounds = u32::try_from(REBUILD_ROUNDS).expect("a page count is a small number");
    let rebuilt = rebuild(&[&reconcile, &replay, &reclaim], rounds)
        .await
        .expect("every recovery pass runs against both live datastores");
    assert!(
        rebuilt.changed > 0,
        "a rebuild over seeded loss must have re-appended or re-marked something: {rebuilt:?}"
    );

    // ── Recovered means OBTAINED: every stranded fleet hands its unfinished
    // event to the next poll, from the ledger, with the dead lease expired.
    let polled_at = clock::now();
    for staged in &stranded {
        let offered = select_within_one_rotation(&leases, &staged.poller, polled_at).await;
        assert!(
            offered.is_some(),
            "fleet {} was marked ready and offers its work",
            staged.fleet
        );
        let acquired = offered.expect("asserted present above");
        assert_eq!(
            acquired.event_id, staged.event_id,
            "the poll re-leases the event the dead holder never finished, not a fresh one"
        );
        let dead = staged
            .lease_id
            .as_deref()
            .expect("a stranded fleet was leased");
        assert_eq!(
            fixtures.lease_column(dead, "status").await.as_deref(),
            Some("expired"),
            "reclaim_prior_active expired the dead holder's lease on the way"
        );
        assert_eq!(
            fixtures.admissions_for(&staged.fleet).await,
            1,
            "recovery re-leases identities; it never mints them"
        );
    }

    // ── A live holder is left alone: not stranded, so not marked, so the
    // other runner's poll finds nothing and the lease it holds is untouched.
    assert!(
        select_within_one_rotation(&leases, &live.poller, polled_at)
            .await
            .is_none(),
        "a fleet whose holder is still inside its lease offers nothing to a second runner"
    );
    let held = live.lease_id.as_deref().expect("the live fleet was leased");
    assert_eq!(
        fixtures.lease_column(held, "status").await.as_deref(),
        Some("active"),
        "the rebuild never touched a lease whose holder is alive"
    );

    // ── Accepted work the stream lost came back as entries, once each.
    for staged in [&queued, &unreceipted] {
        assert_reappended(&fixtures, staged).await;
    }

    for staged in &everyone {
        flush(&fixtures, &streams, &staged.fleet).await;
    }
    fixtures.cleanup().await;
}
