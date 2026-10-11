//! User work, through the real supervisor and agent loop into bubblewrap.

/// Shared tool, call and field spellings in the fixture.
const SCHEDULE: &str = "schedule";
/// Tool-output scrubbing must not hide a credential that reached the child.
const MASKED_SECRET: &str = "«secret:";

use afr_supervisor::config::ENV_RUNNER_TOKEN;
use afr_tools::ToolErrorCode;
use libtest_mimic::Failed;
use serde_json::json;

use crate::lane::Lane;
use crate::run::expect;
use crate::supervisor::{Case, run};
use crate::supervisor_checks::{finished, grafana_sent, supervisor_results};
use crate::supervisor_model::{call, edit_calls, mutations, refused_calls, supervisor_calls};
use crate::supervisor_plane::{PROVIDER_TOKEN, RUNNER_TOKEN, TOKEN};
use crate::trials::Body;

pub(crate) const TRIALS: &[(&str, Body)] = &[
    (
        "test_supervisor_steer_edits_and_commits_inside_bubblewrap",
        edits_and_commits,
    ),
    (
        "test_supervisor_only_tools_need_no_bubblewrap",
        supervisor_only,
    ),
    (
        "test_supervisor_refuses_remote_git_and_unsafe_egress",
        refused,
    ),
    (
        "test_supervisor_does_not_repeat_failed_mutations",
        failed_mutations,
    ),
    (
        "test_supervisor_shutdown_reaps_running_bubblewrap",
        shutdown,
    ),
];

fn edits_and_commits(lane: &Lane) -> Result<(), Failed> {
    let calls = edit_calls();
    let observed = run(
        lane,
        Case {
            calls,
            repository: true,
            ..Case::default()
        },
    )?;
    finished(&observed, 1)?;
    grafana_sent(&observed)?;
    let status = observed.output("confinement")?;
    expect(
        status.contains("CapEff:\t0000000000000000") && status.contains("confined"),
        status,
    )?;
    expect(
        [TOKEN, PROVIDER_TOKEN, RUNNER_TOKEN, MASKED_SECRET]
            .iter()
            .all(|credential| !status.contains(credential))
            && !status.contains(&format!("{ENV_RUNNER_TOKEN}=")),
        "no host credentials or scrubbed credentials in child output",
    )?;
    expect(
        observed.output("read")?.contains("steered fix"),
        "the real file handler reads the child's workspace edit",
    )?;
    expect(
        observed.output("log")?.contains("agentsfleet steered fix"),
        "the real Git handler committed the edit",
    )?;
    expect(
        observed.origin_unchanged,
        "local commit does not publish to the remote",
    )
}

fn supervisor_only(lane: &Lane) -> Result<(), Failed> {
    let calls = supervisor_calls();
    let observed = run(
        lane,
        Case {
            calls,
            ..Case::default()
        },
    )?;
    finished(&observed, 0)?;
    supervisor_results(&observed)?;
    grafana_sent(&observed)
}

fn refused(lane: &Lane) -> Result<(), Failed> {
    let calls = refused_calls();
    let observed = run(
        lane,
        Case {
            calls,
            repository: true,
            ..Case::default()
        },
    )?;
    finished(&observed, 1)?;
    for (id, code) in [
        ("push", ToolErrorCode::SubcommandNotAllowed),
        ("write", ToolErrorCode::MethodNotAllowed),
        ("origin", ToolErrorCode::CredentialHostNotAllowed),
        ("invalid", ToolErrorCode::InvalidArguments),
        (SCHEDULE, ToolErrorCode::InvalidArguments),
    ] {
        let output = observed.output(id)?;
        expect(
            output.contains(code.as_str()),
            format!("{id} carries {code:?}, got {output}"),
        )?;
    }
    expect(
        observed.outbound.is_empty(),
        "refused HTTP calls never reach an upstream",
    )?;
    expect(
        observed.origin_unchanged,
        "refused git push changes no remote ref",
    )?;
    expect(
        !observed
            .received
            .iter()
            .any(|request| request.path.ends_with("/runs")),
        "invalid schedule id reaches no daemon route",
    )
}

fn failed_mutations(lane: &Lane) -> Result<(), Failed> {
    let observed = run(
        lane,
        Case {
            calls: mutations(),
            fail_mutations: true,
            ..Case::default()
        },
    )?;
    finished(&observed, 0)?;
    for (id, suffix) in [(SCHEDULE, "/schedules"), ("message", "/messages")] {
        let attempts = observed
            .received
            .iter()
            .filter(|request| request.path.ends_with(suffix))
            .count();
        expect(
            attempts == 1,
            format!("{id} makes one attempt after a 503, got {attempts}"),
        )?;
        let output = observed.output(id)?;
        expect(
            output.contains(ToolErrorCode::AgentsfleetdUnreachable.as_str()),
            output,
        )?;
    }
    Ok(())
}

fn shutdown(lane: &Lane) -> Result<(), Failed> {
    let calls = vec![call(
        "running",
        "shell",
        json!({ "command": "echo running > running; sleep 300" }),
    )];
    let observed = run(
        lane,
        Case {
            calls,
            cancel_running: true,
            ..Case::default()
        },
    )?;
    expect(
        observed.prepared == 1 && !observed.reaped.is_empty(),
        "shutdown reaped an actual running bubblewrap tenant",
    )
}
