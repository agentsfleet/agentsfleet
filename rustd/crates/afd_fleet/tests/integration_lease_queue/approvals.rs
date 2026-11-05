//! Gate resolution and expiry take the fleet's slot before any admission.

use std::time::Duration;

use afd_admission::Admissions;
use afd_approval::{Decision, Inbox, Resolution};
use afd_crypto::entropy::Entropy;
use afd_db::test_util::mint_id;
use sqlx::Connection as _;

use super::{admit_at, created_at, fence, ready_at};
use crate::rows::Row;
use crate::seed::{claim_fleet, seeded_parts};
use crate::support::Fixtures;

/// The gate kind a rule raised on a tool call, which moves no grant.
const KIND: &str = "tool_call";
/// Who answers the fixture's cards.
const REVIEWER: &str = "fixture:human";
/// A card's window long enough that only an answer ends it.
const OPEN: Duration = Duration::from_secs(3600);
/// How long the case waits for the resolution to queue behind the held slot.
const QUEUED_WITHIN: Duration = Duration::from_secs(10);
/// The statement a waiting session runs, and what holds it up.
const WAITING_ON: &str = "SELECT query FROM pg_stat_activity \
     WHERE $1::integer = ANY(pg_blocking_pids(pid))";

/// Raises one pending card on `fleet`, for `event` or for the whole fleet,
/// open for `window` from now; a zero window is already past its deadline.
pub(super) async fn pending_card(
    fixtures: &Fixtures,
    fleet: &str,
    workspace: &str,
    event: Option<&str>,
    window: Duration,
) -> String {
    let action = mint_id();
    sqlx::query(
        "INSERT INTO fleet.approval_gates \
           (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_type, \
            proposed_action, evidence, blast_radius, expires_at, resolved_by, status, \
            detail, created_at, updated_at, event_id) \
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'chat', 'run', $5, 'run the event', \
                 '{}'::jsonb, 'one fleet', clock_timestamp() + $6::interval - interval '1 second', \
                 'fixture:human', $7, '', now(), now(), $8)",
    )
    .bind(mint_id())
    .bind(fleet)
    .bind(workspace)
    .bind(&action)
    .bind(KIND)
    .bind(window)
    .bind(afd_api_wire::approval::status::PENDING)
    .bind(event)
    .execute(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("the card inserts");
    action
}

pub(super) fn inbox(fixtures: &Fixtures) -> Inbox {
    Inbox::new(
        fixtures.database.clone(),
        fixtures.queue().clone(),
        Admissions::new(
            fixtures.database.clone(),
            fixtures.queue().clone(),
            Entropy::new(),
        ),
    )
}

async fn parked(fixtures: &Fixtures, admission: &str) -> bool {
    fixtures
        .value::<bool>(Row::Admission(admission), "parked_at IS NOT NULL")
        .await
        .expect("the admission exists")
}

/// A park clears the fleet's readiness, and a person's answer
/// puts the head back in its place.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_parked_head_disarms_and_gate_resolution_rearms() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let head = admit_at(&fixtures, &fleet, &workspace, 0).await;
    let action = pending_card(&fixtures, &fleet, &workspace, Some(&head), OPEN).await;
    let leases = fixtures.leases();
    let claimed = claim_fleet(&leases, &runner, &fleet)
        .await
        .expect("claimed");
    leases
        .park(&claimed.fleet_id, &claimed.event_id, claimed.fence)
        .await
        .expect("parked");
    assert_eq!(
        ready_at(&fixtures, &fleet).await,
        None,
        "a parked head is not ready"
    );

    let resolved = inbox(&fixtures)
        .resolve(&action, Decision::Approved, REVIEWER, "", Some(&fleet))
        .await
        .expect("the resolve runs");
    assert!(matches!(resolved, Resolution::Resolved(_)), "{resolved:?}");
    assert_eq!(
        ready_at(&fixtures, &fleet).await,
        created_at(&fixtures, &head).await,
        "the answer re-arms the head in its own place"
    );
    fixtures.cleanup().await;
}

/// Which way the card ends while a park holds the fleet's slot.
#[derive(Clone, Copy, Debug)]
enum Ending {
    Answered,
    Expired,
}

/// A card's ending waits on the fleet's slot, holding no
/// admission row, while a park holds the slot and has stamped the head. Once
/// the park commits, the ending clears the park it raced, with no deadlock.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_gate_resolution_locks_the_slot_before_any_admission() {
    for ending in [Ending::Answered, Ending::Expired] {
        races_a_park(ending).await;
    }
}

async fn races_a_park(ending: Ending) {
    // A private database: the sweep expires every lapsed card it can see.
    let fixtures = Fixtures::create_isolated_with_queue().await;
    let (fleet, workspace, _, [_runner]) = seeded_parts::<1>(&fixtures).await;
    let head = admit_at(&fixtures, &fleet, &workspace, 0).await;
    let window = match ending {
        Ending::Answered => OPEN,
        Ending::Expired => Duration::ZERO,
    };
    let action = pending_card(&fixtures, &fleet, &workspace, Some(&head), window).await;

    // The park's half: the slot, then the head's stamp, uncommitted.
    let mut parking = fixtures.database.acquire().await.expect("a connection");
    let mut park = parking.begin().await.expect("a transaction");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *park)
        .await
        .expect("the session names itself");
    sqlx::query("SELECT 1 FROM runner.fleet_slots WHERE fleet_id = $1::uuid FOR UPDATE")
        .bind(&fleet)
        .execute(&mut *park)
        .await
        .expect("the slot is held");
    sqlx::query("UPDATE fleet.admissions SET parked_at = clock_timestamp() WHERE id = $1::uuid")
        .bind(&head)
        .execute(&mut *park)
        .await
        .expect("the head is stamped");

    let inbox = inbox(&fixtures);
    let scope = fleet.clone();
    let ended = tokio::spawn(async move {
        match ending {
            Ending::Answered => inbox
                .resolve(&action, Decision::Approved, REVIEWER, "", Some(&scope))
                .await
                .map(|resolved| matches!(resolved, Resolution::Resolved(_))),
            Ending::Expired => inbox.expire().await.map(|expired| expired == 1),
        }
    });
    let waiting = waiting_statement(&fixtures, pid).await;
    park.commit().await.expect("the park commits");
    drop(parking);
    let finished = ended.await.expect("the ending joins");
    assert!(
        finished.is_ok(),
        "{ending:?} faulted, no deadlock allowed: {finished:?}"
    );
    let finished = finished.is_ok_and(|ended| ended);

    assert!(
        waiting.contains("runner.fleet_slots") && !waiting.contains("fleet.admissions"),
        "{ending:?} waits on the slot statement, holding no admission: {waiting}"
    );
    assert!(finished, "{ending:?} ended the card");
    assert_eq!(
        (
            parked(&fixtures, &head).await,
            ready_at(&fixtures, &fleet).await
        ),
        (false, created_at(&fixtures, &head).await),
        "{ending:?} cleared the park it raced and re-armed the head"
    );
    fixtures.cleanup().await;
}

/// The statement the session blocked behind `pid` runs, once one is.
async fn waiting_statement(fixtures: &Fixtures, pid: i32) -> String {
    let deadline = tokio::time::Instant::now() + QUEUED_WITHIN;
    loop {
        let waiting: Option<String> = sqlx::query_scalar(WAITING_ON)
            .bind(pid)
            .fetch_optional(&mut *fixtures.database.acquire().await.expect("an observer"))
            .await
            .expect("the waiters read");
        if let Some(statement) = waiting {
            return statement;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "nothing queued behind the held slot"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A card on a fleet that has never had an admission, and so
/// has no slot, still expires and still resolves; each creates the slot at
/// fence 0 with nothing ready.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_setup_card_on_a_fleet_without_admissions_expires() {
    // A private database: the sweep expires every lapsed card it can see.
    let fixtures = Fixtures::create_isolated_with_queue().await;
    let (lapsing, lapsing_ws, _, [_a]) = seeded_parts::<1>(&fixtures).await;
    let (answered, answered_ws, _, [_b]) = seeded_parts::<1>(&fixtures).await;
    let lapsing_card = pending_card(&fixtures, &lapsing, &lapsing_ws, None, Duration::ZERO).await;
    let answered_card = pending_card(&fixtures, &answered, &answered_ws, None, OPEN).await;
    assert_eq!(
        fence(&fixtures, &lapsing).await,
        None,
        "no slot before any work"
    );
    let inbox = inbox(&fixtures);

    let expired = inbox.expire().await.expect("the sweep runs");
    let resolved = inbox
        .resolve(
            &answered_card,
            Decision::Approved,
            REVIEWER,
            "",
            Some(&answered),
        )
        .await
        .expect("the resolve runs");

    let status = |action: String| {
        let fixtures = &fixtures;
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM fleet.approval_gates WHERE action_id = $1",
            )
            .bind(action)
            .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
            .await
            .expect("the card reads")
        }
    };
    assert_eq!(expired, 1, "the lapsed card is swept");
    assert!(matches!(resolved, Resolution::Resolved(_)), "{resolved:?}");
    assert_eq!(
        (
            status(lapsing_card).await,
            fence(&fixtures, &lapsing).await,
            ready_at(&fixtures, &lapsing).await,
            fence(&fixtures, &answered).await,
            ready_at(&fixtures, &answered).await,
        ),
        (
            afd_api_wire::approval::status::TIMED_OUT.to_owned(),
            Some(0),
            None,
            Some(0),
            None,
        )
    );
    fixtures.cleanup().await;
}
