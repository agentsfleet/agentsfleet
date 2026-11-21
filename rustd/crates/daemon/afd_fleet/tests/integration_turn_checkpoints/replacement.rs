//! A replacement uses the actual reassignment and issue paths under a greater fence.

use super::{Case, Checked, required};
use af_core::id::Uuid7;
use afd_fleet::run::{Billed, Kind};

impl Case {
    pub(super) async fn replace(&mut self) -> Checked {
        let mut connection = self.fixtures.database.acquire().await?;
        sqlx::query("UPDATE thread.runs SET run_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
            .bind(&self.issued.run_id).execute(&mut *connection).await?;
        sqlx::query("UPDATE thread.threads SET run_expires_at=clock_timestamp()-interval '1 second' WHERE fleet_id=$1")
            .bind(&self.acquired.fleet_id).execute(&mut *connection).await?;
        let tenant: Uuid7 = sqlx::query_scalar("SELECT tenant_id FROM fleet.fleets WHERE id=$1")
            .bind(&self.acquired.fleet_id)
            .fetch_one(&mut *connection)
            .await?;
        drop(connection);
        let acquired = required(
            crate::seed::select_fleet(&self.runs, &self.foreign, &self.acquired.fleet_id).await,
        )?;
        assert_eq!(acquired.kind, Kind::Reassigned);
        assert!(acquired.fence > self.acquired.fence);
        let installed = required(self.runs.installed(&acquired.fleet_id).await?)?;
        let revision = required(installed.issue_revision)?;
        let issued = required(
            self.runs
                .issue_for_revision(
                    &self.foreign,
                    &acquired,
                    Billed {
                        tenant_id: &tenant,
                        posture: crate::seed::POSTURE,
                        provider: crate::seed::PROVIDER,
                        model: crate::seed::MODEL,
                    },
                    self.fixtures.enrolled_at(),
                    Some(&revision),
                )
                .await?,
        )?;
        std::mem::swap(&mut self.runner, &mut self.foreign);
        self.acquired = acquired;
        self.issued = issued;
        Ok(())
    }
}
