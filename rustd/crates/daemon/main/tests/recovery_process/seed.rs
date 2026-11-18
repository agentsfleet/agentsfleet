//! Configure and exercise one crash-drill fleet.
use super::{
    Checked,
    population::{admit, model, select},
};
use crate::e2e::Scenario;
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use serde_json::json;
pub(super) async fn configure(run: &Scenario, base: &str) -> Checked<(Uuid7, String)> {
    let entry = model(run, base, &run.workspace, &run.tenant).await?;
    let mut connection = run.booted.database.acquire().await?;
    select(&mut connection, &run.fleet, &entry).await?;
    drop(connection);
    runners(run).await
}

async fn runners(run: &Scenario) -> Checked<(Uuid7, String)> {
    let mut connection = run.booted.database.acquire().await?;
    sqlx::query(
        "UPDATE runner.runners SET sandbox_tier=$2,network_policy=$3,durable_tasks_enabled=false,
        registry_allowlist=$4,updated_at=now() WHERE id=$1",
    )
    .bind(&run.runner_id)
    .bind("dev_none")
    .bind("allow_all")
    .bind(sqlx::types::Json(json!([])))
    .execute(&mut *connection)
    .await?;
    drop(connection);
    let mut request = crate::e2e_seed::scenario_enrolment(&run.fleet);
    request.assigned_policy.sandbox_tier = af_wire::runner::SandboxTier::DevNone;
    request.assigned_policy.network_policy = af_wire::runner::NetworkPolicy::AllowAll;
    request.assigned_policy.registry_allowlist.clear();
    request.assigned_policy.durable_tasks_enabled = false;
    let enrolled = afd_runner::Runners::new(run.booted.database.clone(), Entropy::new())
        .register(&request, af_core::clock::now())
        .await?;
    Ok((enrolled.runner_id, enrolled.token.expose().to_owned()))
}

pub(super) async fn fold(run: &Scenario, message: &str) -> Checked<Uuid7> {
    admit(run, &run.fleet, &run.workspace, message).await
}
