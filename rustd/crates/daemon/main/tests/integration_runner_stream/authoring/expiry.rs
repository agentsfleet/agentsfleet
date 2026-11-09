//! Expiry is checked after a host-lock wait, before any physical provider effect.

use super::{DEADLINE, fixture::AuthoringFixture};
use af_core::{clock, error_code::AUTHORING_FENCE, id::Uuid7};
use agentsfleetd::supervisor::Supervisor;
use jiff_sqlx::ToSqlx as _;
use sqlx::{Postgres, Transaction};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_model_call_waiting_on_the_host_lock_cannot_use_its_pre_wait_expiry_check() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, false).await;
    let (host, blocker) = hold_host(&fixture).await;
    let until = clock::after(clock::now(), Duration::from_secs(2));
    sqlx::query("UPDATE authoring.attempts SET lease_expires_at=$2 WHERE lease_id=$1")
        .bind(fixture.id())
        .bind(until.to_sqlx())
        .execute(
            &mut *fixture
                .booted
                .database
                .acquire()
                .await
                .expect("lease expiry connection"),
        )
        .await
        .expect("bound the original lease expiry");
    let lease = fixture.id();
    let model = fixture.model();
    let (result, ()) = tokio::join!(fixture.session.authoring_model(&lease, &model), async {
        wait_for_host_wait(&fixture, blocker).await;
        tokio::time::sleep(Duration::from_millis(2_100)).await;
        host.commit()
            .await
            .expect("release the host after lease expiry");
    });
    assert_eq!(
        result.expect_err("expiry after host-lock wait").code(),
        AUTHORING_FENCE
    );
    assert_eq!(fixture.provider.state.count(), 0);
    assert_eq!(fixture.receipts().await, (0, 0, 0));
    fixture.finish(supervisor).await;
}

async fn hold_host(fixture: &AuthoringFixture) -> (Transaction<'static, Postgres>, i32) {
    let mut host = Transaction::begin(
        fixture
            .booted
            .database
            .acquire()
            .await
            .expect("host lock connection"),
        None,
    )
    .await
    .expect("host lock transaction");
    // Identify this test's lock holder, so an unrelated database wait cannot satisfy the test.
    let blocker = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *host)
        .await
        .expect("host lock backend");
    let id: Uuid7 = sqlx::query_scalar("SELECT id FROM runner.runners WHERE id=$1 FOR UPDATE")
        .bind(&fixture.runner)
        .fetch_one(&mut *host)
        .await
        .expect("hold native runner row");
    assert_eq!(id, fixture.runner);
    (host, blocker)
}

async fn wait_for_host_wait(fixture: &AuthoringFixture, blocker: i32) {
    tokio::time::timeout(DEADLINE, async {
        loop {
            let waiting_on_host: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(blocker)
            .fetch_one(
                &mut *fixture
                    .booted
                    .database
                    .acquire()
                    .await
                    .expect("lock observer connection"),
            )
            .await
            .expect("observe the real host-lock wait");
            if waiting_on_host {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the native call reached the held host lock");
}
