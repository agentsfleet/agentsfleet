//! The real fleet issuer counts parked holds and rolls their transfer back on refusal.

use crate::{queue, requests::placement_tag, seed, support::Fixtures};
use af_core::id::Uuid7;
use afd_fleet::lease::{Acquired, Billed, Issued, Leases};

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct Case {
    fixtures: Fixtures,
    leases: Leases,
    runner: Uuid7,
    tenant: Uuid7,
    warm: Uuid7,
    cold: Acquired,
}

impl Case {
    async fn new() -> Checked<Self> {
        let fixtures = Fixtures::create().await;
        let seeded = seed::seeded::<1>(&fixtures).await;
        let [runner] = seeded.runners;
        let leases = fixtures.leases();
        let mut connection = fixtures.database.acquire().await?;
        sqlx::query("UPDATE runner.runners SET worker_count=2 WHERE id=$1")
            .bind(&runner)
            .execute(&mut *connection)
            .await?;
        let active = required(seed::select_fleet(&leases, &runner, &seeded.fleet).await)?;
        let billed = Billed {
            tenant_id: &seeded.tenant,
            posture: seed::POSTURE,
            provider: seed::PROVIDER,
            model: seed::MODEL,
        };
        required(
            leases
                .issue(&runner, &active, billed, fixtures.enrolled_at())
                .await?,
        )?;
        let warm = extra_fleet(&fixtures, &active, &seeded.tenant).await;
        let cold_fleet = extra_fleet(&fixtures, &active, &seeded.tenant).await;
        let cold = required(seed::select_fleet(&leases, &runner, &cold_fleet).await)?;
        sqlx::query("INSERT INTO runner.slots
            (id,fleet_id,last_runner_id,fencing_token,lease_expires_at,hold_expires_at,created_at,updated_at)
            VALUES ($1,$1,$2,1,clock_timestamp(),clock_timestamp()+interval '60 seconds',now(),now())")
            .bind(&warm).bind(&runner).execute(&mut *connection).await?;
        drop(connection);
        Ok(Self {
            fixtures,
            leases,
            runner,
            tenant: seeded.tenant,
            warm,
            cold,
        })
    }

    async fn issue(&self, acquired: &Acquired) -> Checked<Option<Issued>> {
        Ok(self
            .leases
            .issue(
                &self.runner,
                acquired,
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

    async fn stamp(&self) -> Checked<Stamp> {
        let mut connection = self.fixtures.database.acquire().await?;
        Ok(sqlx::query_as("SELECT s.hold_expires_at,
            (SELECT count(*) FROM runner.leases l WHERE l.runner_id=$1 AND l.status='active') AS active,
            (SELECT count(*) FROM runner.events e WHERE e.runner_id=$1) AS events,
            r.lease_version AS choice, COALESCE(c.acquired_count,0) AS acquired
            FROM runner.slots s JOIN runner.runners r ON r.id=$1
            LEFT JOIN runner.lifetime_counters c ON c.runner_id=r.id WHERE s.fleet_id=$2")
            .bind(&self.runner).bind(&self.warm).fetch_one(&mut *connection).await?)
    }
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct Stamp {
    hold_expires_at: Option<jiff_sqlx::Timestamp>,
    active: i64,
    events: i64,
    choice: i64,
    acquired: i64,
}

fn required<T>(value: Option<T>) -> Checked<T> {
    value.ok_or_else(|| "complete issue-capacity fixture".into())
}

async fn extra_fleet(fixtures: &Fixtures, active: &Acquired, tenant: &Uuid7) -> Uuid7 {
    let fleet = afd_db::test_util::mint_id();
    fixtures
        .seed_fleet(
            &fleet,
            &active.workspace_id,
            tenant,
            &placement_tag(&active.fleet_id),
        )
        .await;
    queue::enqueue(
        &fixtures.database,
        &fleet,
        &active.workspace_id,
        seed::ACTOR,
        seed::EVENT_TYPE,
        seed::REQUEST_JSON,
        fixtures.enrolled_at(),
    )
    .await;
    fleet
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_fleet_issuer_refuses_a_third_worker_and_transfers_its_own_hold_once() -> Checked {
    let case = Case::new().await?;
    let before = case.stamp().await?;
    assert_eq!(before.active, 1);
    assert!(before.hold_expires_at.is_some());
    assert!(case.issue(&case.cold).await?.is_none());
    assert_eq!(case.stamp().await?, before);
    let warm = required(seed::select_fleet(&case.leases, &case.runner, &case.warm).await)?;
    assert!(warm.resume_hold);
    required(case.issue(&warm).await?)?;
    let after = case.stamp().await?;
    assert!(after.hold_expires_at.is_none());
    assert_eq!(after.active, 2);
    assert_eq!(after.events, before.events + 1);
    assert_eq!(after.choice, before.choice + 1);
    assert_eq!(after.acquired, before.acquired + 1);
    assert!(case.issue(&case.cold).await?.is_none());
    assert_eq!(case.stamp().await?, after);
    case.fixtures.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_failed_task_source_attach_restores_the_exact_idle_hold_and_issue_tallies() -> Checked {
    let case = Case::new().await?;
    let warm = required(seed::select_fleet(&case.leases, &case.runner, &case.warm).await)?;
    let before = case.stamp().await?;
    let mut connection = case.fixtures.database.acquire().await?;
    // Deleting a task clears its foreign key while retaining its claimed generation.
    sqlx::query("UPDATE fleet.admissions SET task_generation=1 WHERE id=$1")
        .bind(&warm.event_id)
        .execute(&mut *connection)
        .await?;
    assert!(case.issue(&warm).await?.is_none());
    assert_eq!(
        case.stamp().await?,
        before,
        "attach refusal rolls back the consumed hold, lease, choice and counters"
    );
    sqlx::query("UPDATE fleet.admissions SET task_generation=NULL WHERE id=$1")
        .bind(&warm.event_id)
        .execute(&mut *connection)
        .await?;
    required(case.issue(&warm).await?)?;
    let after = case.stamp().await?;
    assert!(after.hold_expires_at.is_none());
    assert_eq!(after.active, 2);
    assert_eq!(after.acquired, before.acquired + 1);
    drop(connection);
    case.fixtures.cleanup().await;
    Ok(())
}
