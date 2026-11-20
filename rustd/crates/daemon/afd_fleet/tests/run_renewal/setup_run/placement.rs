//! A tenant's runner is never assigned a setup run that carries the platform
//! model's key: a draft turn on the platform default waits for a platform
//! host, and the same runner takes it once the turn names the tenant's own
//! model.

use af_core::id::Uuid7;
use af_wire::runner::{NetworkPolicy, SandboxTier};
use afd_db::test_util::mint_id;

use super::Case;
use crate::requests::{enrolment_tagged, placement_tag};
use crate::seed::reports_capable;
use crate::support::Fixtures;

// Value: protects=the platform key never reaches a tenant's host; fails_when=the
// head's model test is dropped from both the draft scan and the draft assignment;
// why_new=the draft run path had no tenant-host case; seam=the production draft
// assignment over live Postgres, polled by a runner the tenant registered.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_tenant_runner_is_not_assigned_a_platform_key_setup_run() {
    let case = Case::queued().await;
    let runner = tenant_runner(&case.running.fixtures, &case.seat.tenant).await;
    let own = case.turn_model().await;
    assert!(
        own.is_some(),
        "the seat's turn starts on the tenant's model"
    );
    // `None` is the platform default: its key is the platform's own.
    case.set_turn_model(None).await;

    for poll in 1..=2 {
        assert!(
            case.take(&runner).await.is_none(),
            "poll {poll}: a platform-default turn is not the tenant host's"
        );
    }
    assert_eq!(case.runs().await, Vec::new(), "no run written");
    let queued = case.turn_on_head().await;
    assert_eq!(queued.status, "queued", "the turn waits: {queued:?}");
    assert_eq!(queued.failure_reason, None, "and is not failed: {queued:?}");

    case.set_turn_model(own.as_ref()).await;
    let taken = case
        .take(&runner)
        .await
        .expect("the same host takes the turn on the tenant's own model");
    assert_eq!(taken.fence, 1);
    case.cleanup().await;
}

/// A runner `tenant` registered, which has reported capable.
async fn tenant_runner(fixtures: &Fixtures, tenant: &Uuid7) -> Uuid7 {
    let request = enrolment_tagged(
        SandboxTier::LandlockFull,
        NetworkPolicy::AllowListEgress,
        1,
        &placement_tag(&mint_id()),
    );
    let registered = fixtures
        .runners()
        .register_tenant(
            tenant,
            &request,
            "fixture:tenant-owner",
            fixtures.enrolled_at(),
        )
        .await
        .expect("the tenant registers its runner");
    let runner = registered.runner_id;
    reports_capable(fixtures, &runner).await;
    runner
}

impl Case {
    /// The model the draft's one turn names: `None` is the platform default.
    async fn turn_model(&self) -> Option<Uuid7> {
        sqlx::query_scalar("SELECT tenant_model_id FROM thread.turns WHERE thread_id = $1")
            .bind(&self.draft.id)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the draft's one turn")
    }

    /// Points the draft's one turn at `model`, as a message sent on it would.
    async fn set_turn_model(&self, model: Option<&Uuid7>) {
        let set = sqlx::query("UPDATE thread.turns SET tenant_model_id = $2 WHERE thread_id = $1")
            .bind(&self.draft.id)
            .bind(model)
            .execute(&mut *self.connection().await)
            .await
            .expect("the turn's model is set");
        assert_eq!(set.rows_affected(), 1, "the draft holds one turn");
    }

    /// The draft's one turn, as it stands.
    async fn turn_on_head(&self) -> super::TurnRow {
        let id: Uuid7 = sqlx::query_scalar("SELECT id FROM thread.turns WHERE thread_id = $1")
            .bind(&self.draft.id)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the draft's one turn");
        self.turn(&id).await
    }
}
