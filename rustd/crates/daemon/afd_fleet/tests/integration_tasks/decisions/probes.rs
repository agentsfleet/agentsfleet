//! Complete native resource and financial observations shared by scenarios.

use afd_fleet::task::{TaskLifecycle as _, TaskSnapshot};
use afd_wire::task::DecisionTaskVersion;

use super::{super::fixture::id, fixture::Decisions};

impl Decisions<'_> {
    pub(super) async fn goal_snapshot(&self, task: &DecisionTaskVersion) -> TaskSnapshot {
        self.tasks
            .read(&self.case.scope, &id(&task.task_id))
            .await
            .expect("native goal snapshot")
    }

    pub(super) async fn wallet(&self) -> i64 {
        sqlx::query_scalar("SELECT w.balance_nanos FROM billing.wallets w JOIN runner.leases l ON l.tenant_id=w.tenant_id WHERE l.id=$1::uuid")
            .bind(self.case.held.lease_key.as_str())
            .fetch_one(&mut *self.case.held.fixtures.database.acquire().await.expect("original payer observer"))
            .await.expect("original wallet balance")
    }
}
