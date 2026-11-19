//! Against a live datastore: a tenant's runner takes only its
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
use afd_fleet::run::{Acquired, Billed, Runs};
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
    let registered = fixtures
        .runners()
        .register_tenant(
            tenant,
            &mint_id(),
            &request,
            "fixture:tenant-owner",
            fixtures.enrolled_at(),
        )
        .await
        .expect("the tenant registers its runner");
    assert!(
        matches!(registered, Registered::Created(_)),
        "a fresh key registers a new runner"
    );
    let runner = match registered {
        Registered::Created(enrolled) => enrolled.runner_id,
        Registered::Replayed { runner_id, .. } => runner_id,
    };
    reports_capable(fixtures, &runner).await;
    runner
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
        .register(
            &request,
            "fixture:platform-operator",
            fixtures.enrolled_at(),
        )
        .await
        .expect("the platform registers its runner");
    reports_capable(fixtures, &enrolled.runner_id).await;
    enrolled.runner_id
}

/// Issues `held` to `runner`, billed to `tenant` under `posture`.
async fn issue(
    fixtures: &Fixtures,
    runs: &Runs,
    runner: &Uuid7,
    held: &Acquired,
    tenant: &Uuid7,
    posture: &str,
) -> bool {
    runs.issue(
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

/// Runs ever issued to `runner`.
async fn runs_of(fixtures: &Fixtures, runner: &Uuid7) -> i64 {
    scalar(
        fixtures,
        "SELECT count(*) FROM thread.runs WHERE runner_id = $1",
        runner,
    )
    .await
}

/// Whether `fleet` has a slot row, which only a winning claim writes.
async fn slotted(fixtures: &Fixtures, fleet: &Uuid7) -> bool {
    scalar(
        fixtures,
        "SELECT count(*) FROM thread.threads WHERE fleet_id = $1",
        fleet,
    )
    .await
        > 0
}

/// `fleet`'s slot as a claim leaves it: its fence, deadline and holder.
async fn slot(fixtures: &Fixtures, fleet: &Uuid7) -> (i64, String, Option<Uuid7>) {
    sqlx::query_as(
        "SELECT fencing_token, run_expires_at::text, last_runner_id FROM thread.threads WHERE fleet_id = $1",
    )
    .bind(fleet)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("the slot reads")
}

/// Lapses `fleet`'s claim, as an expired run leaves it.
async fn lapse(fixtures: &Fixtures, fleet: &Uuid7) {
    sqlx::query(
        "UPDATE thread.threads SET run_expires_at = now() - interval '1 hour', hold_expires_at = NULL WHERE fleet_id = $1",
    )
    .bind(fleet)
    .execute(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("the claim lapses");
}

/// The claim statement alone, bound as the ledger binds it; whether it claimed.
async fn claim_head(fixtures: &Fixtures, fleet: &Uuid7, runner: &Uuid7) -> bool {
    use afd_fleet::run::sql;
    use sqlx::Connection as _;
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    let mut transaction = connection.begin().await.expect("a claim transaction");
    let claimed = sqlx::query(sql::queue::CLAIM_HEAD)
        .bind(fleet)
        .bind(runner)
        .bind(af_core::timing::RUN_TTL)
        .bind(af_core::timing::RUNNER_OFFLINE_AFTER)
        .bind(sql::RUNNER_STATUS_ACTIVE)
        .bind(sql::RUN_STATUS_ACTIVE)
        .bind(sql::RUN_STATUS_EXPIRED)
        .bind(af_wire::task::CAPABILITY)
        .bind(af_wire::task::TaskState::Ready.as_str())
        .bind(af_wire::task::TaskState::Running.as_str())
        .bind(afd_fleet::task::DECISION_POLICY_PATH)
        .bind(sql::FLEET_STATUS_ACTIVE)
        .bind(af_core::turn::status::RECEIVED)
        .bind(afd_turns::Producer::Steer.as_str())
        .bind(true)
        .bind(afd_credential::provider::PLATFORM_MODEL_REF)
        .fetch_optional(&mut *transaction)
        .await
        .expect("the claim answers")
        .is_some();
    // As the ledger does: a claim that took no head rolls its slot write back.
    if claimed {
        transaction.commit().await.expect("the claim commits");
    } else {
        transaction
            .rollback()
            .await
            .expect("the refused claim rolls back");
    }
    claimed
}

const SELF_MANAGED: &str = afd_billing::sql::posture::SELF_MANAGED;
const PLATFORM: &str = afd_billing::sql::posture::PLATFORM;

mod ownership;
mod platform_model;
mod waiting;
