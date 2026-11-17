//! §3 and §4 of M246 against a live datastore: a tenant's runner takes only its
//! tenant's work, never work that carries the platform model key, and the claim
//! and the issue each refuse on their own.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_wire::runner::{NetworkPolicy, SandboxTier};
use afd_db::test_util::mint_id;
use afd_fleet::lease::{Acquired, Billed, Leases};
use afd_runner::Registered;

use crate::requests::{enrolment_tagged, placement_tag};
use crate::seed::{ACTOR, PROVIDER, REQUEST_JSON, reports_capable, select_fleet};
use crate::support::Fixtures;

/// A tenant's own model: anything but the platform default.
const TENANT_MODEL: &str = "tenant-own-model";

/// One tenant's workspace and the tag its fleets and runners share.
struct Owner {
    tenant: Uuid7,
    workspace: Uuid7,
    tag: String,
}

impl Owner {
    /// A tenant whose first fleet creates it; `tag` keeps this test's fleets apart.
    fn new() -> Self {
        Self {
            tenant: mint_id(),
            workspace: mint_id(),
            tag: placement_tag(&mint_id()),
        }
    }

    /// A fleet on `model` with one queued admission.
    async fn fleet(&self, fixtures: &Fixtures, model: &str) -> Uuid7 {
        let fleet = mint_id();
        fixtures
            .seed_fleet_on(&fleet, &self.workspace, &self.tenant, &self.tag, model)
            .await;
        crate::queue::enqueue(
            &fixtures.database,
            &fleet,
            &self.workspace,
            ACTOR,
            "chat",
            REQUEST_JSON,
            fixtures.enrolled_at(),
        )
        .await;
        fleet
    }
}

/// A runner `tenant` registered, carrying `tag`, that has reported capable.
async fn tenant_runner(fixtures: &Fixtures, tenant: &Uuid7, tag: &str) -> Uuid7 {
    let request = enrolment_tagged(
        SandboxTier::LandlockFull,
        NetworkPolicy::AllowListEgress,
        2,
        tag,
    );
    let Registered::Created(enrolled) = fixtures
        .runners()
        .register_tenant(tenant, &mint_id(), &request, fixtures.enrolled_at())
        .await
        .expect("the tenant registers its runner")
    else {
        panic!("a fresh key registers a new runner");
    };
    reports_capable(fixtures, &enrolled.runner_id).await;
    enrolled.runner_id
}

/// A platform runner carrying `tag`, that has reported capable.
async fn platform_runner(fixtures: &Fixtures, tag: &str) -> Uuid7 {
    let request = enrolment_tagged(
        SandboxTier::LandlockFull,
        NetworkPolicy::AllowListEgress,
        2,
        tag,
    );
    let enrolled = fixtures
        .runners()
        .register(&request, fixtures.enrolled_at())
        .await
        .expect("the platform registers its runner");
    reports_capable(fixtures, &enrolled.runner_id).await;
    enrolled.runner_id
}

/// Issues `held` to `runner`, billed to `tenant` under `posture`.
async fn issue(
    fixtures: &Fixtures,
    leases: &Leases,
    runner: &Uuid7,
    held: &Acquired,
    tenant: &Uuid7,
    posture: &str,
) -> bool {
    leases
        .issue(
            runner,
            held,
            Billed {
                tenant_id: tenant,
                posture,
                provider: PROVIDER,
                model: TENANT_MODEL,
            },
            fixtures.enrolled_at(),
        )
        .await
        .expect("the issue answers")
        .is_some()
}

async fn scalar(fixtures: &Fixtures, statement: &'static str, id: &Uuid7) -> i64 {
    sqlx::query_scalar(statement)
        .bind(id)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the count reads")
}

/// Leases ever issued to `runner`.
async fn leases_of(fixtures: &Fixtures, runner: &Uuid7) -> i64 {
    scalar(
        fixtures,
        "SELECT count(*) FROM runner.leases WHERE runner_id = $1",
        runner,
    )
    .await
}

/// Whether `fleet` has a slot row, which only a winning claim writes.
async fn slotted(fixtures: &Fixtures, fleet: &Uuid7) -> bool {
    scalar(
        fixtures,
        "SELECT count(*) FROM runner.fleet_slots WHERE fleet_id = $1",
        fleet,
    )
    .await
        > 0
}

/// `fleet`'s slot as a claim leaves it: its fence, deadline and holder.
async fn slot(fixtures: &Fixtures, fleet: &Uuid7) -> (i64, String, Option<Uuid7>) {
    sqlx::query_as(
        "SELECT fencing_token, lease_expires_at::text, last_runner_id FROM runner.fleet_slots WHERE fleet_id = $1",
    )
    .bind(fleet)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("the slot reads")
}

/// Lapses `fleet`'s claim, as an expired lease leaves it.
async fn lapse(fixtures: &Fixtures, fleet: &Uuid7) {
    sqlx::query(
        "UPDATE runner.fleet_slots SET lease_expires_at = now() - interval '1 hour', hold_expires_at = NULL WHERE fleet_id = $1",
    )
    .bind(fleet)
    .execute(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("the claim lapses");
}

/// The claim statement alone, bound as the ledger binds it; whether it claimed.
async fn claim_head(fixtures: &Fixtures, fleet: &Uuid7, runner: &Uuid7) -> bool {
    use afd_fleet::lease::sql;
    sqlx::query(sql::queue::CLAIM_HEAD)
        .bind(fleet)
        .bind(runner)
        .bind(af_core::timing::LEASE_TTL)
        .bind(af_core::timing::RUNNER_OFFLINE_AFTER)
        .bind(sql::RUNNER_STATUS_ACTIVE)
        .bind(sql::LEASE_STATUS_ACTIVE)
        .bind(sql::LEASE_STATUS_EXPIRED)
        .bind(af_wire::task::CAPABILITY)
        .bind(af_wire::task::TaskState::Ready.as_str())
        .bind(af_wire::task::TaskState::Running.as_str())
        .bind(afd_fleet::task::DECISION_POLICY_PATH)
        .bind(sql::FLEET_STATUS_ACTIVE)
        .bind(af_core::event::status::RECEIVED)
        .bind(afd_admission::Producer::Steer.as_str())
        .bind(afd_credential::provider::PLATFORM_MODEL_REF)
        .fetch_optional(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the claim answers")
        .is_some()
}

const SELF_MANAGED: &str = afd_billing::sql::posture::SELF_MANAGED;
const PLATFORM: &str = afd_billing::sql::posture::PLATFORM;

/// Dimension 3.1: the live matrix matches the rule.
// Value: protects=a tenant runner takes only its tenant's work and a platform
// runner any; fails_when=the scan or claim drops the owner, or refuses
// everything; why_new=§3's filter; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn cross_tenant_placement_matrix() {
    let fixtures = Fixtures::create().await;
    let leases = fixtures.leases();
    let t = Owner::new();
    let [own, shared, foreign] = [
        t.fleet(&fixtures, TENANT_MODEL).await,
        t.fleet(&fixtures, TENANT_MODEL).await,
        t.fleet(&fixtures, TENANT_MODEL).await,
    ];
    let u = fixtures.seed_tenant().await;
    let t_runner = tenant_runner(&fixtures, &t.tenant, &t.tag).await;
    let p_runner = platform_runner(&fixtures, &t.tag).await;
    let u_runner = tenant_runner(&fixtures, &u, &t.tag).await;

    let held = select_fleet(&leases, &t_runner, &own)
        .await
        .expect("T's runner takes T's work");
    assert!(
        issue(
            &fixtures,
            &leases,
            &t_runner,
            &held,
            &t.tenant,
            SELF_MANAGED
        )
        .await
    );
    let held = select_fleet(&leases, &p_runner, &shared)
        .await
        .expect("a platform runner takes any tenant's");
    assert!(
        issue(
            &fixtures,
            &leases,
            &p_runner,
            &held,
            &t.tenant,
            SELF_MANAGED
        )
        .await
    );

    assert!(
        select_fleet(&leases, &u_runner, &foreign).await.is_none(),
        "U's runner sees no candidate"
    );
    assert!(!slotted(&fixtures, &foreign).await, "and writes no slot");
    assert_eq!(
        leases_of(&fixtures, &u_runner).await,
        0,
        "and holds no lease"
    );
    fixtures.cleanup().await;
}

/// Dimension 3.2: the claim alone refuses a foreign runner.
// Value: protects=the claim statement is the control, not the scan;
// fails_when=the claim's `fleet` step stops joining the runner; why_new=§3;
// seam=none.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn claim_statement_refuses_a_foreign_runner() {
    let fixtures = Fixtures::create().await;
    let t = Owner::new();
    let fleet = t.fleet(&fixtures, TENANT_MODEL).await;
    let u = fixtures.seed_tenant().await;
    let t_runner = tenant_runner(&fixtures, &t.tenant, &t.tag).await;
    let u_runner = tenant_runner(&fixtures, &u, &t.tag).await;

    assert!(
        claim_head(&fixtures, &fleet, &t_runner).await,
        "T's runner claims"
    );
    lapse(&fixtures, &fleet).await;
    let before = slot(&fixtures, &fleet).await;
    assert!(
        !claim_head(&fixtures, &fleet, &u_runner).await,
        "U's runner is refused"
    );
    assert_eq!(
        slot(&fixtures, &fleet).await,
        before,
        "the slot, its fence and deadline are untouched"
    );
    assert!(
        claim_head(&fixtures, &fleet, &t_runner).await,
        "and T's runner still claims"
    );
    assert_eq!(
        slot(&fixtures, &fleet).await.0,
        before.0 + 1,
        "with the next fence"
    );
    fixtures.cleanup().await;
}

/// Dimension 3.3: issue refuses a foreign runner under the host lock.
// Value: protects=issue holds even when the claim lets a placement through;
// fails_when=the host lock stops binding the work's tenant; why_new=§3's
// re-check; seam=an Acquired handed straight to issue.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn issue_refuses_a_foreign_runner_under_the_host_lock() {
    let fixtures = Fixtures::create().await;
    let leases = fixtures.leases();
    let t = Owner::new();
    let fleet = t.fleet(&fixtures, TENANT_MODEL).await;
    let u = fixtures.seed_tenant().await;
    let t_runner = tenant_runner(&fixtures, &t.tenant, &t.tag).await;
    let u_runner = tenant_runner(&fixtures, &u, &t.tag).await;
    let held = select_fleet(&leases, &t_runner, &fleet)
        .await
        .expect("T's runner claims");

    let captured = af_core::test_util::trace::Capture::install();
    let issued = issue(
        &fixtures,
        &leases,
        &u_runner,
        &held,
        &t.tenant,
        SELF_MANAGED,
    )
    .await;
    let refused = captured.only("runner_placement_refused");
    drop(captured);

    assert!(!issued, "a foreign host gets no lease");
    assert_eq!(leases_of(&fixtures, &u_runner).await, 0);
    assert_eq!(refused.field("error_code"), Some("UZ-RUN-027"));
    assert_eq!(refused.field("reason"), Some("foreign_tenant"));
    assert_eq!(refused.field("lease_type"), Some("fleet"));
    fixtures.cleanup().await;
}

/// Dimension 4.1, placement half: work on the platform model never reaches a
/// tenant runner, and a platform runner still takes it.
// Value: protects=the platform key stays on platform hosts; fails_when=the
// head's model test is dropped from the scan or the claim; why_new=§4;
// seam=none.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn platform_model_work_never_reaches_a_tenant_runner() {
    let fixtures = Fixtures::create().await;
    let leases = fixtures.leases();
    let t = Owner::new();
    let fleet = t
        .fleet(&fixtures, afd_credential::provider::PLATFORM_MODEL_REF)
        .await;
    let t_runner = tenant_runner(&fixtures, &t.tenant, &t.tag).await;

    for poll in 1..=2 {
        assert!(
            select_fleet(&leases, &t_runner, &fleet).await.is_none(),
            "poll {poll}: no candidate"
        );
    }
    assert!(!slotted(&fixtures, &fleet).await, "no slot write");
    assert!(
        !claim_head(&fixtures, &fleet, &t_runner).await,
        "the claim alone refuses too"
    );
    assert_eq!(leases_of(&fixtures, &t_runner).await, 0, "no lease");

    let p_runner = platform_runner(&fixtures, &t.tag).await;
    let held = select_fleet(&leases, &p_runner, &fleet)
        .await
        .expect("a platform runner takes it");
    assert!(issue(&fixtures, &leases, &p_runner, &held, &t.tenant, PLATFORM).await);
    fixtures.cleanup().await;
}

/// Dimension 4.1, issue half: a tenant host is refused platform-posture work
/// under the host lock, even when handed it directly.
// Value: protects=issue's posture re-check; fails_when=`tenant_owned` stops
// reaching issue; why_new=§4's re-check; seam=an Acquired handed straight to issue.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn issue_refuses_platform_posture_on_a_tenant_runner() {
    let fixtures = Fixtures::create().await;
    let leases = fixtures.leases();
    let t = Owner::new();
    let fleet = t.fleet(&fixtures, TENANT_MODEL).await;
    let t_runner = tenant_runner(&fixtures, &t.tenant, &t.tag).await;
    let held = select_fleet(&leases, &t_runner, &fleet)
        .await
        .expect("T's runner claims");

    let captured = af_core::test_util::trace::Capture::install();
    let issued = issue(&fixtures, &leases, &t_runner, &held, &t.tenant, PLATFORM).await;
    let refused = captured.only("runner_placement_refused");
    drop(captured);

    assert!(!issued, "platform posture never issues to a tenant host");
    assert_eq!(leases_of(&fixtures, &t_runner).await, 0);
    assert_eq!(refused.field("reason"), Some("platform_model"));
    fixtures.cleanup().await;
}
