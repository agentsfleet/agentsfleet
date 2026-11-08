//! A selected admission cannot acquire execution after its fleet was killed.

use std::time::Duration;

use af_core::id::Uuid7;
use afd_fleet::lease::{Acquired, Billed, Issued};
use sqlx::{Connection as _, PgConnection};

use crate::{
    seed::{self, Seeded},
    support::Fixtures,
};

type TestResult<T = ()> = afd_fleet::Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct Selected {
    fixtures: Fixtures,
    runner: Uuid7,
    tenant: Uuid7,
    acquired: Acquired,
}

impl Selected {
    async fn create() -> TestResult<Self> {
        let fixtures = Fixtures::create_with_queue().await;
        let Seeded {
            runners: [runner],
            fleet,
            tenant,
            ..
        } = seed::seeded::<1>(&fixtures).await;
        let leases = fixtures.leases();
        let acquired = seed::select_fleet(&leases, &runner, &fleet)
            .await
            .ok_or("expected queued work")?;
        leases.record_received(&acquired).await?;
        Ok(Self {
            fixtures,
            runner,
            tenant,
            acquired,
        })
    }

    async fn issue(&self) -> TestResult<Option<Issued>> {
        Ok(self
            .fixtures
            .leases()
            .issue(
                &self.runner,
                &self.acquired,
                Billed {
                    tenant_id: &self.tenant,
                    posture: seed::POSTURE,
                    provider: seed::PROVIDER,
                    model: seed::MODEL,
                },
                self.fixtures.enrolled_at(),
            )
            .await?)
    }

    async fn kill(&self, connection: &mut PgConnection) -> TestResult {
        sqlx::query("UPDATE fleet.fleets SET status = $2 WHERE id = $1")
            .bind(&self.acquired.fleet_id)
            .bind(afd_api_wire::fleet::PatchStatus::Killed.as_str())
            .execute(connection)
            .await?;
        Ok(())
    }

    async fn assert_empty(&self) -> TestResult {
        let leases: i64 =
            sqlx::query_scalar("SELECT count(*) FROM runner.leases WHERE runner_id = $1")
                .bind(&self.runner)
                .fetch_one(&mut *self.fixtures.database.acquire().await?)
                .await?;
        assert_eq!(leases, 0, "a rejected issue writes no lease");
        Ok(())
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_kill_committed_after_selection_prevents_a_later_issue() -> TestResult {
    let selected = Selected::create().await?;
    selected
        .kill(&mut *selected.fixtures.database.acquire().await?)
        .await?;
    assert!(
        selected.issue().await?.is_none(),
        "Kill invalidated the selected work"
    );
    selected.assert_empty().await?;
    selected.fixtures.cleanup().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_issue_waiting_on_a_kill_rechecks_the_committed_fleet_state() -> TestResult {
    let selected = Selected::create().await?;
    let mut writer = selected.fixtures.database.acquire().await?;
    let mut transaction = writer.begin().await?;
    let blocker: i32 =
        sqlx::query_scalar("SELECT pg_backend_pid() FROM fleet.fleets WHERE id = $1 FOR UPDATE")
            .bind(&selected.acquired.fleet_id)
            .fetch_one(&mut *transaction)
            .await?;
    let killing = async {
        wait_for_blocker(&selected, blocker).await?;
        selected.kill(&mut transaction).await?;
        transaction.commit().await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    };
    let (issued, killed) = tokio::join!(selected.issue(), killing);
    killed?;
    assert!(
        issued?.is_none(),
        "an issue cannot outlive the Kill it waited behind"
    );
    selected.assert_empty().await?;
    drop(writer);
    selected.fixtures.cleanup().await;
    Ok(())
}

async fn wait_for_blocker(selected: &Selected, blocker: i32) -> TestResult {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut observer = selected.fixtures.database.acquire().await?;
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        loop {
            tick.tick().await;
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))")
                .bind(blocker).fetch_one(&mut *observer).await?;
            if waiting { return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(()); }
        }
    }).await?
}
