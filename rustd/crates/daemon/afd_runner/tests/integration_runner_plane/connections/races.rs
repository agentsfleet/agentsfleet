//! Assertions wait for actual PostgreSQL blocking, rather than elapsed sleeps.

use std::time::Duration;

use af_core::clock;
use afd_auth::directory::Digest;
use afd_db::Db;
use afd_runner::capacity::authority::{Decision, Refusal};
use sqlx::Connection as _;

use super::{Fixture, TestResult, acquire, can_claim, enrol};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn rotation_during_a_host_lock_wait_cannot_authorize_old_credentials() -> TestResult {
    for opening in [false, true] {
        let mut fixture = Fixture::create().await;
        let (runner, digest) = enrol(&mut fixture).await?;
        let authority = acquire(&fixture, &runner, &digest, None).await?;
        let replacement =
            Digest::of_minted(&format!("rotation-fixture-{}", authority.connection()));
        let mut writer = fixture.owner.acquire().await?;
        let mut held = writer.begin().await?;
        let blocker: i32 = sqlx::query_scalar(
            "SELECT pg_backend_pid() FROM runner.runners WHERE id = $1 FOR UPDATE",
        )
        .bind(&runner)
        .fetch_one(&mut *held)
        .await?;
        let waiting = async {
            if opening {
                let decision = fixture
                    .runners
                    .acquire_connection(&runner, digest, None, clock::now())
                    .await?;
                assert_eq!(decision, Decision::Refused(Refusal::Credential));
            } else {
                assert!(!can_claim(&fixture, &authority).await?);
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        };
        let rotating = async {
            wait_for_blocker(&fixture.owner, blocker).await?;
            sqlx::query("UPDATE runner.runners SET token_hash = $2 WHERE id = $1")
                .bind(&runner)
                .bind(replacement.as_str())
                .execute(&mut *held)
                .await?;
            held.commit().await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        };
        tokio::try_join!(waiting, rotating)?;
        assert_eq!(
            fixture.runners.check_connection(&authority).await?,
            Decision::Refused(Refusal::Credential)
        );
        let current = acquire(&fixture, &runner, &replacement, None).await?;
        assert!(can_claim(&fixture, &current).await?);
        drop(writer);
        fixture.cleanup().await;
    }
    Ok(())
}

async fn wait_for_blocker(database: &Db, blocker: i32) -> TestResult {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut connection = database.acquire().await?;
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        loop {
            tick.tick().await;
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))")
                .bind(blocker).fetch_one(&mut *connection).await?;
            if waiting { return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(()); }
        }
    }).await?
}
