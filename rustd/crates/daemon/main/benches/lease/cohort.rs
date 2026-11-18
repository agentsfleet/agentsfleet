//! Enrol compatible source/replacement hosts and seed real ordinary admissions.

use super::{Checked, e2e::Scenario, e2e_seed, model::Model, process::Worker, seed};
use af_core::{
    env::{EnvSource as _, ProcessEnv},
    id::Uuid7,
};
use af_crypto::entropy::Entropy;
use afd_db::test_util::mint_id;
use std::{env::current_exe, time::Duration};

pub(super) struct Fleet {
    pub id: Uuid7,
    pub workspace: Uuid7,
    pub event: Uuid7,
}

pub(super) async fn population(run: &Scenario, model: &Model, n: usize) -> Checked<Vec<Fleet>> {
    let layout = ProcessEnv
        .get("BENCH_WORKSPACES")
        .unwrap_or_else(|| "shared".into());
    if !matches!(layout.as_str(), "shared" | "distributed") {
        return Err("BENCH_WORKSPACES must be shared or distributed".into());
    }
    let entry = seed::model(run, &model.base(), &run.workspace, &run.tenant).await?;
    {
        let mut conn = run.booted.database.acquire().await?;
        seed::select(&mut conn, &run.fleet, &entry).await?;
    }
    let mut items = vec![Fleet {
        id: run.fleet.clone(),
        workspace: run.workspace.clone(),
        event: run.event_id.clone(),
    }];
    for index in 1..n {
        let (id, tenant, workspace) = if layout == "shared" {
            (mint_id(), run.tenant.clone(), run.workspace.clone())
        } else {
            (mint_id(), mint_id(), mint_id())
        };
        e2e_seed::seed_fleet(
            &run.booted,
            &id,
            &format!("burst-{index}"),
            &workspace,
            &tenant,
            e2e_seed::FLEET_CONFIG_JSON,
            run.seeded_at,
        )
        .await;
        let selected = if layout == "shared" {
            entry.clone()
        } else {
            e2e_seed::seed_wallet(&run.booted, &tenant, e2e_seed::DEEP_POOL, run.seeded_at).await;
            seed::model(run, &model.base(), &workspace, &tenant).await?
        };
        match_template(run, &id, &selected).await?;
        let event = seed::admit(run, &id, &workspace, "Begin the recovery check.").await?;
        items.push(Fleet {
            id,
            workspace,
            event,
        });
    }
    Ok(items)
}

async fn match_template(run: &Scenario, fleet: &Uuid7, model: &Uuid7) -> Checked {
    let mut conn = run.booted.database.acquire().await?;
    sqlx::query("UPDATE fleet.fleets SET required_tags=(SELECT required_tags FROM fleet.fleets WHERE id=$2) WHERE id=$1")
        .bind(fleet).bind(&run.fleet).execute(&mut *conn).await?;
    seed::select(&mut conn, fleet, model).await
}

pub(super) async fn hosts(
    run: &Scenario,
    model: &Model,
    n: usize,
) -> Checked<(Vec<Worker>, Vec<Uuid7>)> {
    let mut workers = Vec::new();
    let mut ids = Vec::new();
    for start in (0..n).step_by(64) {
        let mut req = e2e_seed::scenario_enrolment(&run.fleet);
        req.host_id = format!("burst-{}", mint_id()).into();
        req.assigned_policy.worker_count = (n - start).min(64).try_into()?;
        req.assigned_policy.durable_tasks_enabled = false;
        req.assigned_policy.sandbox_tier = af_wire::runner::SandboxTier::DevNone;
        req.assigned_policy.network_policy = af_wire::runner::NetworkPolicy::AllowAll;
        req.assigned_policy.registry_allowlist.clear();
        let host = afd_runner::Runners::new(run.booted.database.clone(), Entropy::new())
            .register(&req, af_core::clock::now())
            .await?;
        workers.push(Worker::start_with(
            &current_exe()?,
            run,
            host.token.expose(),
            model,
        )?);
        ids.push(host.runner_id);
    }
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            let mut conn = run.booted.database.acquire().await?;
            let connected: i64 = sqlx::query_scalar("SELECT count(*) FROM runner.runners WHERE id=ANY($1::uuid[]) AND connection_id IS NOT NULL AND degraded_reason IS NULL AND capability_report IS NOT NULL")
                .bind(&ids).fetch_one(&mut *conn).await?;
            if usize::try_from(connected)? == ids.len() { return Ok::<_, Box<dyn std::error::Error+Send+Sync>>(()); }
            drop(conn);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await??;
    Ok((workers, ids))
}
