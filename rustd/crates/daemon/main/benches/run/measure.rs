//! Source turns stop at a real committed root. Replacement answers stay behind a barrier.

use super::{
    Checked,
    cohort::{self, Fleet},
    e2e::{self, Scenario},
    model::{Asked, Model},
    process::Worker,
    seed,
};
use af_core::{
    clock,
    env::{EnvSource as _, ProcessEnv},
    id::Uuid7,
};
use agentsfleetd::supervisor::Supervisor;
use futures_util::future::try_join_all;
use serde_json::{Value, json};
use std::io::Write as _;
use std::{collections::BTreeMap, time::Duration};

const ACCEPTED: &str = "The source accepted this recovery checkpoint.";
const MARKER: &str = "burst-fleet/";
const WINDOW_MS: i64 = 60_000;

pub(super) async fn run(n: usize) -> Checked {
    let mut daemon = Supervisor::new();
    let run = e2e::scenario(&mut daemon).await;
    let mut model = Model::start().await?;
    let fleets = cohort::population(&run, &model, n).await?;
    stage(format_args!("{n} source fleets prepared"))?;
    let (source, source_ids) = cohort::hosts(&run, &model, n).await?;
    stage("source runners connected")?;
    let interrupted = checkpoint(&run, &fleets, &mut model).await?;
    stage("all source checkpoints committed")?;
    let (survivors, survivor_ids) = cohort::hosts(&run, &model, n).await?;
    stage("replacement runners connected")?;
    tokio::time::sleep(af_core::timing::OFFER_FALLBACK_TICK).await;
    let _logs = try_join_all(source.into_iter().map(Worker::kill)).await?;
    let eligible = eligibility(&run, &fleets, &source_ids).await?;
    stage("runners stopped; waiting for expiry")?;
    drop(interrupted);
    let resumed = observe(&mut model, &eligible).await?;
    let mut raw = Vec::new();
    for (fleet, requested) in &eligible {
        let ms = resumed.get(fleet).map(|(at, _)| at - requested);
        raw.push(json!({"fleet":fleet, "eligible_unix_ms":requested, "first_model_ms":ms}));
    }
    for (_, (_, asked)) in resumed {
        asked.answer("Finished the recovered turn.")?;
    }
    let issued = outcomes(&run, &fleets, &survivor_ids, &eligible, &raw).await?;
    let _logs = try_join_all(survivors.into_iter().map(Worker::kill)).await?;
    let _stopped = daemon.shutdown().await;
    for fleet in fleets.iter().skip(1) {
        super::e2e_retire::retire_fleet(&run.booted, &fleet.id).await;
    }
    run.cleanup().await;
    model.finish().await?;
    writeln!(
        std::io::stdout(),
        "{}",
        serde_json::to_string(&json!({
            "shape":"recovery", "workload":"api_only", "fleets":n, "hosts_per_group":n.div_ceil(64),
            "workspaces":ProcessEnv.get("BENCH_WORKSPACES").unwrap_or_else(|| "shared".into()),
            "revision":ProcessEnv.get("BENCH_REVISION"), "window_from_last_expiry_ms":WINDOW_MS,
            "fleet_samples":raw, "replacement_runs":issued
        }))?
    )?;
    Ok(())
}

fn stage(message: impl std::fmt::Display) -> Checked {
    writeln!(std::io::stderr(), "recovery benchmark: {message}")?;
    Ok(())
}

async fn checkpoint(run: &Scenario, fleets: &[Fleet], model: &mut Model) -> Checked<Vec<Asked>> {
    let mut initial = Vec::new();
    for _ in fleets {
        initial.push(model.next().await?);
    }
    for fleet in fleets {
        seed::admit(
            run,
            &fleet.id,
            &fleet.workspace,
            &format!("{MARKER}{}", fleet.id),
        )
        .await?;
    }
    for asked in initial {
        asked.answer(ACCEPTED)?;
    }
    let mut next = Vec::new();
    for _ in fleets {
        next.push(model.next().await?);
    }
    let events: Vec<_> = fleets.iter().map(|f| f.event.clone()).collect();
    let mut conn = run.booted.database.acquire().await?;
    let heads: i64 = sqlx::query_scalar("SELECT count(*) FROM runner.turn_recovery h JOIN runner.turn_checkpoints p ON p.id=h.head_turn_checkpoint_id WHERE h.turn_id=ANY($1::uuid[]) AND p.ready_at IS NOT NULL AND p.root_secret_ciphertext IS NOT NULL")
        .bind(&events).fetch_one(&mut *conn).await?;
    if usize::try_from(heads)? != fleets.len() {
        return Err("source cohort lacks a committed root".into());
    }
    Ok(next)
}

async fn eligibility(
    run: &Scenario,
    fleets: &[Fleet],
    sources: &[Uuid7],
) -> Checked<BTreeMap<Uuid7, i64>> {
    let events: Vec<_> = fleets.iter().map(|f| f.event.clone()).collect();
    let mut conn = run.booted.database.acquire().await?;
    let rows: Vec<(Uuid7,i64)> = sqlx::query_as("SELECT l.fleet_id,(extract(epoch from greatest(l.run_expires_at,s.run_expires_at))*1000)::bigint FROM thread.runs l JOIN thread.threads s ON s.fleet_id=l.fleet_id AND l.fencing_token=s.fencing_token WHERE l.turn_id=ANY($1::uuid[]) AND l.runner_id=ANY($2::uuid[])")
        .bind(&events).bind(sources).fetch_all(&mut *conn).await?;
    let eligible: BTreeMap<_, _> = rows.into_iter().collect();
    if eligible.len() != fleets.len() {
        return Err("cohort changed ownership before expiry snapshot".into());
    }
    Ok(eligible)
}

async fn observe(
    model: &mut Model,
    eligible: &BTreeMap<Uuid7, i64>,
) -> Checked<BTreeMap<Uuid7, (i64, Asked)>> {
    let end = eligible.values().max().ok_or("empty cohort")? + WINDOW_MS;
    let duration = Duration::from_millis(u64::try_from(end - clock::now().as_millisecond())?);
    let deadline = tokio::time::Instant::now() + duration;
    let mut resumed = BTreeMap::new();
    while resumed.len() < eligible.len() {
        let Ok(asked) = tokio::time::timeout_at(deadline, model.next_timed()).await else {
            break;
        };
        let (at, asked) = asked?;
        let text = serde_json::to_string(asked.body.get("messages").ok_or("missing messages")?)?;
        let (_, tail) = text
            .split_once(MARKER)
            .ok_or("resumed model missing incorporated fold")?;
        let fleet = Uuid7::parse(tail.get(..36).ok_or("short fleet marker")?)?;
        let expiry = eligible.get(&fleet).ok_or("unknown fleet at model entry")?;
        if at.as_millisecond() < *expiry
            || text.matches(ACCEPTED).count() != 1
            || text.matches(MARKER).count() != 1
        {
            return Err("early model request or incorrect restored prefix".into());
        }
        if resumed
            .insert(fleet, (at.as_millisecond(), asked))
            .is_some()
        {
            return Err("duplicate resumed model call".into());
        }
    }
    Ok(resumed)
}

async fn outcomes(
    run: &Scenario,
    fleets: &[Fleet],
    hosts: &[Uuid7],
    eligible: &BTreeMap<Uuid7, i64>,
    raw: &[Value],
) -> Checked<Vec<Value>> {
    let events: Vec<_> = fleets.iter().map(|f| f.event.clone()).collect();
    let expected = raw
        .iter()
        .filter(|r| r["first_model_ms"].is_number())
        .count();
    let began = tokio::time::Instant::now();
    loop {
        let mut conn = run.booted.database.acquire().await?;
        let rows:Vec<(Uuid7,i64,i64,Option<i64>,String)>=sqlx::query_as("SELECT l.fleet_id,l.fencing_token,(extract(epoch from l.created_at)*1000)::bigint,(extract(epoch from a.settled_at)*1000)::bigint,l.status FROM thread.runs l JOIN thread.turns a ON a.id=l.turn_id WHERE l.turn_id=ANY($1::uuid[]) AND l.runner_id=ANY($2::uuid[]) ORDER BY l.fleet_id,l.fencing_token")
            .bind(&events).bind(hosts).fetch_all(&mut *conn).await?;
        if rows.iter().filter(|r| r.3.is_some()).count() >= expected
            || began.elapsed() > Duration::from_secs(10)
        {
            return rows.into_iter().map(|(fleet,fence,issued,settled,status)| {
                let at=eligible.get(&fleet).ok_or("unknown replacement run")?;
                Ok(json!({"fleet":fleet,"fence":fence,"issue_ms":issued-at,"settle_ms":settled.map(|s|s-at),"status":status}))
            }).collect();
        }
        drop(conn);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
