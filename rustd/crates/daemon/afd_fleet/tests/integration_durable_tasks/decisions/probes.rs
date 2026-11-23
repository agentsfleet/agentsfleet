//! Complete native resource and financial observations shared by scenarios.

use af_wire::durable_task::DecisionDurableTaskVersion;
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTaskSnapshot};

use super::{super::fixture::id, fixture::Decisions};

impl Decisions<'_> {
    pub(super) async fn goal_snapshot(
        &self,
        task: &DecisionDurableTaskVersion,
    ) -> DurableTaskSnapshot {
        self.tasks
            .read(&self.case.scope, &id(&task.durable_task_id))
            .await
            .expect("native goal snapshot")
    }

    pub(super) async fn wallet(&self) -> i64 {
        sqlx::query_scalar("SELECT w.balance_nanos FROM billing.wallets w JOIN thread.runs l ON l.tenant_id=w.tenant_id WHERE l.id=$1")
            .bind(&self.case.held.run_key)
            .fetch_one(&mut *self.case.held.fixtures.database.acquire().await.expect("original payer observer"))
            .await.expect("original wallet balance")
    }
}
