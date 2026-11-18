//! A healthy-return proof survives neither invalid observations nor stale authority.

use super::{Fixture, assert_proof_reset, beat, connected, prove_elapsed, quarantined};
use af_core::{clock, id::Uuid7};
use af_wire::runner::{CapabilityReport, HeartbeatRequest};
use afd_api_wire::admin::RunnerEventType;
use afd_auth::directory::Digest;
use afd_runner::capacity::authority::{Authority, Decision, Refusal};
use sqlx::{Connection as _, PgConnection};
use std::time::Duration;

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn malformed_capability_cannot_complete_a_healthy_return() -> Checked {
    let (fixture, owner) = ready().await;
    let request = HeartbeatRequest {
        capability_report: Some(CapabilityReport {
            durable_tasks_v1: true,
            build_id: Some("invalid".into()),
            landlock: true,
            seccomp: true,
            cgroup_controllers: Vec::new(),
            bubblewrap: true,
            egress_enforcement: true,
        }),
        ..afd_runner::NO_REPORT
    };
    assert!(matches!(
        fixture
            .runners
            .heartbeat_connected(&owner, &request, None, clock::now(),)
            .await?,
        Decision::Allowed(_)
    ));
    assert_proof_reset(&fixture, owner.runner()).await;
    no_lift(&fixture, owner.runner()).await;
    fixture.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_degraded_capability_resets_the_healthy_return() -> Checked {
    let (fixture, owner) = ready().await;
    sqlx::query("UPDATE runner.runners SET sandbox_tier=$2 WHERE id=$1")
        .bind(owner.runner())
        .bind("landlock_full")
        .execute(&mut *fixture.owner.acquire().await?)
        .await?;
    let Decision::Allowed(reply) = fixture
        .runners
        .heartbeat_connected(&owner, &afd_runner::NO_REPORT, None, clock::now())
        .await?
    else {
        return Err("a degraded observation must still record liveness".into());
    };
    assert!(reply.verdict.is_degraded());
    assert_proof_reset(&fixture, owner.runner()).await;
    no_lift(&fixture, owner.runner()).await;
    fixture.cleanup().await;
    Ok(())
}

#[derive(Clone, Copy)]
enum Change {
    Assignment,
    Connection,
    Credential,
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn assignment_changed_after_policy_read_resets_healthy_return() -> Checked {
    racing(Change::Assignment).await
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn supersession_after_policy_read_cannot_advance_or_lift_healthy_return() -> Checked {
    racing(Change::Connection).await
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn rotation_after_policy_read_cannot_advance_or_lift_healthy_return() -> Checked {
    racing(Change::Credential).await
}

async fn ready() -> (Fixture, Authority) {
    let mut fixture = Fixture::isolated().await;
    let owner = connected(&mut fixture).await;
    quarantined(&fixture, owner.runner()).await;
    beat(&fixture, &owner).await;
    prove_elapsed(&fixture, owner.runner()).await;
    (fixture, owner)
}

async fn racing(change: Change) -> Checked {
    let (fixture, owner) = ready().await;
    let before = health(&fixture, owner.runner()).await?;
    let mut writer = fixture.owner.acquire().await?;
    let mut held = writer.begin().await?;
    let blocker: i32 =
        sqlx::query_scalar("SELECT pg_backend_pid() FROM runner.runners WHERE id=$1 FOR UPDATE")
            .bind(owner.runner())
            .fetch_one(&mut *held)
            .await?;
    let mutate = async {
        wait_behind(&fixture, blocker).await?;
        update(&mut held, owner.runner(), change).await?;
        held.commit().await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    };
    let request = afd_runner::NO_REPORT;
    let (reply, changed) = tokio::join!(
        fixture
            .runners
            .heartbeat_connected(&owner, &request, None, clock::now()),
        mutate,
    );
    changed?;
    match change {
        Change::Assignment => {
            assert!(matches!(reply?, Decision::Allowed(_)));
            assert_proof_reset(&fixture, owner.runner()).await;
        }
        Change::Connection | Change::Credential => {
            assert!(matches!(reply?, Decision::Refused(Refusal::Superseded)));
            assert_eq!(health(&fixture, owner.runner()).await?, before);
        }
    }
    no_lift(&fixture, owner.runner()).await;
    drop(writer);
    fixture.cleanup().await;
    Ok(())
}

async fn health(fixture: &Fixture, runner: &Uuid7) -> Checked<serde_json::Value> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_array(last_seen_at,cordon_healthy_since_at,cordon_reason,cordoned_at)
         FROM runner.runners WHERE id=$1",
    )
    .bind(runner)
    .fetch_one(&mut *fixture.owner.acquire().await?)
    .await?)
}

async fn update(connection: &mut PgConnection, runner: &Uuid7, change: Change) -> Checked {
    match change {
        Change::Assignment => {
            sqlx::query("UPDATE runner.runners SET worker_count=2 WHERE id=$1")
                .bind(runner)
                .execute(connection)
                .await?;
        }
        Change::Connection => {
            sqlx::query("UPDATE runner.runners SET connection_id=$2 WHERE id=$1")
                .bind(runner)
                .bind(afd_db::test_util::mint_id())
                .execute(connection)
                .await?;
        }
        Change::Credential => {
            let digest = Digest::of_minted(&format!("rotated-return-proof-{runner}"));
            sqlx::query("UPDATE runner.runners SET token_hash=$2 WHERE id=$1")
                .bind(runner)
                .bind(digest.as_str())
                .execute(connection)
                .await?;
        }
    }
    Ok(())
}

async fn wait_behind(fixture: &Fixture, blocker: i32) -> Checked {
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut observer = fixture.owner.acquire().await?;
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            ).bind(blocker).fetch_one(&mut *observer).await?;
            if waiting { return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(()); }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await?
}

async fn no_lift(fixture: &Fixture, runner: &Uuid7) {
    assert!(
        fixture
            .history(runner)
            .await
            .iter()
            .all(|event| event.event_type != RunnerEventType::RunnerUncordoned,)
    );
}
