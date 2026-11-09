//! Registry lifetime and delayed registration follow the durable host owner.

use super::Registry;
use crate::session::SessionHandle;
use af_core::{clock, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::runner::{AssignedPolicy, NetworkPolicy, RegisterRequest, SandboxTier};
use afd_db::{config::DbRole, test_util::TestDatabase};
use afd_runner::{
    Runners,
    capacity::authority::{Authority, Decision},
};
use std::sync::Arc;

type TestResult = crate::Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[test]
fn dropping_an_old_registration_cannot_remove_its_replacement() -> TestResult {
    let registry = Arc::new(Registry::default());
    let runner = Uuid7::parse("019f0000-0000-7000-8000-000000000001")?;
    let a = SessionHandle::fixture(
        runner.clone(),
        Uuid7::parse("019f0000-0000-7000-8000-000000000002")?,
    );
    let b = SessionHandle::fixture(
        runner,
        Uuid7::parse("019f0000-0000-7000-8000-000000000003")?,
    );
    let first = registry.register(Arc::clone(&a))?;
    let second = registry.register(Arc::clone(&b))?;
    drop(first);
    assert!(a.is_superseded());
    assert!(!b.is_superseded());
    assert_eq!(registry.candidates(&Default::default()).len(), 1);
    drop(second);
    assert!(registry.candidates(&Default::default()).is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_delayed_old_owner_cannot_register_over_the_new_owner() -> TestResult {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let owner = lane.open(DbRole::Migrator, &[]).await;
    let runners = Runners::new(database.clone(), Entropy::new());
    let request = RegisterRequest {
        host_id: "registry-registration.test".into(),
        labels: Vec::new(),
        assigned_policy: AssignedPolicy {
            durable_tasks_enabled: true,
            decision_model_id: None,
            sandbox_tier: SandboxTier::DevNone,
            network_policy: NetworkPolicy::AllowAll,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        },
    };
    let enrolled = runners.register(&request, clock::now()).await?;
    let runner = &enrolled.runner_id;
    let a = allowed(
        runners
            .acquire_connection(runner, enrolled.token.digest().clone(), None, clock::now())
            .await?,
    )?;
    // B commits and registers while A is delayed rebuilding its lease cache.
    let b = allowed(
        runners
            .acquire_connection(runner, enrolled.token.digest().clone(), None, clock::now())
            .await?,
    )?;
    let registry = Arc::new(Registry::default());
    let current = SessionHandle::fixture(runner.clone(), b.connection().clone());
    let registration = registry
        .register_current(Arc::clone(&current), &b, &database)
        .await?;
    let delayed = SessionHandle::fixture(runner.clone(), a.connection().clone());
    let refused = registry.register_current(delayed, &a, &database).await;
    assert!(refused.is_err_and(|error| error.code() == af_core::error_code::RUN_STREAM_SUPERSEDED));
    assert!(!current.is_superseded());
    let candidates = registry.candidates(&Default::default());
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].connection, *b.connection());
    drop(registration);
    sqlx::query("DELETE FROM runner.runners WHERE id = $1")
        .bind(runner)
        .execute(&mut *owner.acquire().await?)
        .await?;
    database.close().await;
    owner.close().await;
    lane.cleanup().await;
    Ok(())
}

fn allowed(
    decision: Decision<Authority>,
) -> crate::Result<Authority, Box<dyn std::error::Error + Send + Sync>> {
    match decision {
        Decision::Allowed(authority) => Ok(authority),
        Decision::Refused(_) => Err("expected a current owner".into()),
    }
}
