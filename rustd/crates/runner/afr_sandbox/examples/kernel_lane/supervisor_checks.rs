//! Assertions over the real runner's prompt, fenced requests and HTTPS calls.

/// Shared tool, call and field spellings in the fixture.
const POST: &str = "POST";
const FENCING_FIELD: &str = "fencing_token";

use afd_wire::paths::{
    LEASE_SCHEDULES_SUFFIX, RUNNER_LEASES, RUNNER_MEMORY, RUNNER_REPORTS, SCHEDULE_RUNS_SUFFIX,
};
use afr_supervisor::test_util::{FENCING, LEASE_ID};
use libtest_mimic::Failed;
use serde_json::json;

use crate::run::expect;
use crate::supervisor::Observed;
use crate::supervisor_model::{ANSWER, INSTRUCTIONS, STEER};
use crate::supervisor_plane::{GRAFANA, Received, SCHEDULE_EVENT_ID, SCHEDULE_ID, TOKEN};

pub(crate) fn received<'a>(observed: &'a Observed, suffix: &str) -> Result<&'a Received, Failed> {
    observed
        .received
        .iter()
        .find(|request| request.path.ends_with(suffix))
        .ok_or_else(|| format!("no daemon request ending in {suffix}").into())
}

pub(crate) fn finished(observed: &Observed, preparations: usize) -> Result<(), Failed> {
    expect(
        observed.prepared == preparations,
        format!(
            "{preparations} actual preparations, got {}",
            observed.prepared
        ),
    )?;
    let asked = observed.asked.first().ok_or_else(|| {
        format!(
            "the real loop reaches the model; reports: {:?}",
            observed
                .received
                .iter()
                .filter(|request| request.path == RUNNER_REPORTS)
                .collect::<Vec<_>>()
        )
    })?;
    expect(
        asked.instructions.contains(INSTRUCTIONS) && asked.user.iter().any(|user| user == STEER),
        format!("installed instructions and user steer reach the model: {asked:?}"),
    )?;
    let report = received(observed, RUNNER_REPORTS)?;
    expect(
        report.body.get("outcome") == Some(&json!("processed")),
        format!("processed report, got {}", report.body),
    )?;
    expect(
        report.body.to_string().contains(ANSWER),
        "the final model answer reaches the report",
    )?;
    fenced(observed)
}

fn fenced(observed: &Observed) -> Result<(), Failed> {
    for request in &observed.received {
        let mutation = request.path.ends_with("/schedules")
            || request.path.ends_with("/messages")
            || request.path.ends_with("/runs");
        if mutation {
            expect(
                request.method == POST
                    && request
                        .path
                        .starts_with(&format!("{RUNNER_LEASES}/{LEASE_ID}/")),
                format!("mutation bound to the fixture lease: {}", request.path),
            )?;
        }
        if mutation
            || request.path == RUNNER_REPORTS
            || request.method == POST && request.path.starts_with(RUNNER_MEMORY)
        {
            expect(
                request.body.get(FENCING_FIELD) == Some(&json!(FENCING)),
                format!("fenced request to {}", request.path),
            )?;
        }
    }
    Ok(())
}

pub(crate) fn grafana_sent(observed: &Observed) -> Result<(), Failed> {
    let [sent] = observed.outbound.as_slice() else {
        return Err(format!("one Grafana request, got {}", observed.outbound.len()).into());
    };
    expect(
        sent.method == hyper::Method::GET && sent.host == GRAFANA && sent.path == "/api/search",
        "Grafana host placeholder resolved",
    )?;
    expect(
        sent.headers
            .get(hyper::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            == Some(format!("Bearer {TOKEN}").as_str()),
        "only the allowed upstream receives its credential",
    )?;
    expect(
        observed
            .asked
            .iter()
            .all(|asked| !format!("{asked:?}").contains(TOKEN)),
        "model transcript carries no integration credential",
    )
}

pub(crate) fn supervisor_results(observed: &Observed) -> Result<(), Failed> {
    expect(
        observed.output("recall")?.contains("earlier fleet finding"),
        "memory recall reaches the daemon",
    )?;
    expect(
        observed.output("message")?.contains("posted to the thread"),
        "the daemon acknowledges the thread message",
    )?;
    schedules_sent(observed)?;
    let capture = observed
        .received
        .iter()
        .position(|request| {
            request.path.starts_with(RUNNER_MEMORY)
                && request.method == POST
                && request.body.to_string().contains("kernel caller verified")
        })
        .ok_or("stored memory reaches a fenced capture")?;
    let report = observed
        .received
        .iter()
        .position(|request| request.path == RUNNER_REPORTS)
        .ok_or("the runner reports its answer")?;
    expect(
        capture < report,
        "memory capture completes before the final report",
    )?;
    expect(
        observed
            .output("grafana")?
            .contains("grafana fixture dashboard"),
        "the guarded Grafana answer reaches the model",
    )?;
    Ok(())
}

fn schedules_sent(observed: &Observed) -> Result<(), Failed> {
    let schedule = received(observed, "/schedules")?;
    expect(
        schedule.body.get("cron") == Some(&json!("0 9 * * 1")),
        "cron expression forwarded",
    )?;
    expect(
        schedule.body.get("timezone") == Some(&json!("Asia/Kolkata")),
        "schedule timezone forwarded",
    )?;
    expect(
        observed.output("schedule")?.contains(SCHEDULE_ID),
        "the created schedule identifier reaches the model",
    )?;
    let path = format!(
        "{RUNNER_LEASES}/{LEASE_ID}/{LEASE_SCHEDULES_SUFFIX}/{SCHEDULE_ID}/{SCHEDULE_RUNS_SUFFIX}"
    );
    let run = received(observed, &path)?;
    expect(
        run.method == POST && run.body == json!({FENCING_FIELD: FENCING}),
        "run-now targets the created schedule with the lease's fence",
    )?;
    expect(
        observed.output("run")?.contains(SCHEDULE_EVENT_ID),
        "the admitted run's event identifier reaches the model",
    )
}
