//! The issue counts the host inside its own insert (M245 §5): the host lock
//! stays its own statement, and the census, the worker limit and the placement
//! choice ride the guarded lease insert, so a full host writes nothing.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_fleet::lease::{Acquired, Billed, Leases};

use crate::requests::placement_tag;
use crate::seed::{ACTOR, MODEL, POSTURE, PROVIDER, REQUEST_JSON, seeded_parts, select_fleet};
use crate::support::Fixtures;

/// The host's live leases.
const LIVE: &str = "SELECT count(*) FROM runner.leases \
    WHERE runner_id = $1 AND status = 'active' AND lease_expires_at > now()";

/// The host's placement choice revision.
const VERSION: &str = "SELECT lease_version FROM runner.runners WHERE id = $1";

/// `count` more fleets in `workspace`, each holding one event, all placeable
/// on the runners tagged for `tagged`.
async fn fleets(
    fixtures: &Fixtures,
    tagged: &Uuid7,
    workspace: &Uuid7,
    tenant: &Uuid7,
    count: usize,
) -> Vec<Uuid7> {
    let tag = placement_tag(tagged);
    let mut fleets = Vec::with_capacity(count);
    for _ in 0..count {
        let fleet = afd_db::test_util::mint_id();
        fixtures.seed_fleet(&fleet, workspace, tenant, &tag).await;
        crate::queue::enqueue(
            &fixtures.database,
            &fleet,
            workspace,
            ACTOR,
            "chat",
            REQUEST_JSON,
            fixtures.enrolled_at(),
        )
        .await;
        fleets.push(fleet);
    }
    fleets
}

async fn issue(
    fixtures: &Fixtures,
    leases: &Leases,
    runner: &Uuid7,
    held: &Acquired,
    tenant: &Uuid7,
) -> bool {
    leases
        .issue(
            runner,
            held,
            Billed {
                tenant_id: tenant,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            fixtures.enrolled_at(),
        )
        .await
        .expect("the issue answers")
        .is_some()
}

async fn scalar(fixtures: &Fixtures, statement: &'static str, runner: &Uuid7) -> i64 {
    sqlx::query_scalar(statement)
        .bind(runner)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("the host reads")
}

async fn set_workers(fixtures: &Fixtures, runner: &Uuid7, workers: i32) {
    sqlx::query("UPDATE runner.runners SET worker_count = $2 WHERE id = $1")
        .bind(runner)
        .bind(workers)
        .execute(&mut *fixtures.database.acquire().await.expect("operator"))
        .await
        .expect("the host's worker limit is assigned");
}

// Value: protects=a full host takes no lease; fails_when=the census leaves the
// insert or reads a snapshot from before the host lock; why_new=the census
// moved into the insert; seam=none; no_claim=authoring attempts' share.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn issue_refuses_full_host_in_insert() {
    let fixtures = Fixtures::create().await;
    let (first, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let [second] = fleets(&fixtures, &first, &workspace, &tenant, 1)
        .await
        .try_into()
        .expect("one more fleet");
    crate::queue::enqueue(
        &fixtures.database,
        &first,
        &workspace,
        ACTOR,
        "chat",
        REQUEST_JSON,
        fixtures.enrolled_at(),
    )
    .await;
    set_workers(&fixtures, &runner, 1).await;
    let leases = fixtures.leases_with_dead_bus();
    let one = select_fleet(&leases, &runner, &first)
        .await
        .expect("the first fleet is claimed");
    let two = select_fleet(&leases, &runner, &second)
        .await
        .expect("the second fleet is claimed while the host is empty");

    assert!(issue(&fixtures, &leases, &runner, &one, &tenant).await);
    let version = scalar(&fixtures, VERSION, &runner).await;
    assert!(
        !issue(&fixtures, &leases, &runner, &two, &tenant).await,
        "the full host refuses the second lease"
    );
    assert_eq!(scalar(&fixtures, LIVE, &runner).await, 1);
    assert_eq!(
        scalar(&fixtures, VERSION, &runner).await,
        version,
        "a refused insert moves no placement choice"
    );
    fixtures.cleanup().await;
}

// Value: protects=racing issues never overfill a host; fails_when=two issues
// count the same census; why_new=the census and the insert are one statement
// behind the host lock; seam=none; no_claim=renewal's own count.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn concurrent_issues_never_overfill_a_host() {
    const ISSUERS: usize = 100;
    const WORKERS: i32 = 4;
    let fixtures = Fixtures::create().await;
    let (first, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let all = fleets(&fixtures, &first, &workspace, &tenant, ISSUERS).await;
    set_workers(&fixtures, &runner, WORKERS).await;
    let leases = fixtures.leases_with_dead_bus();
    let mut claimed = Vec::with_capacity(ISSUERS);
    for fleet in &all {
        claimed.push(
            select_fleet(&leases, &runner, fleet)
                .await
                .expect("every fleet is claimed before any issue"),
        );
    }

    let issued = futures_util::future::join_all(
        claimed
            .iter()
            .map(|held| issue(&fixtures, &leases, &runner, held, &tenant)),
    )
    .await;

    let won = issued.iter().filter(|issued| **issued).count();
    assert_eq!(won, usize::try_from(WORKERS).expect("a small limit"));
    assert_eq!(scalar(&fixtures, LIVE, &runner).await, i64::from(WORKERS));
    fixtures.cleanup().await;
}
