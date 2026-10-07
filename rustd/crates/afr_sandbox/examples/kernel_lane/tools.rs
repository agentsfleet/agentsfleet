//! The sandbox-side tools in a real sandbox: each trial calls the handler a
//! lease's model calls, through the catalog the runner hosts, over the
//! executor bubblewrap serves.

use std::sync::Arc;

use afr_egress::testing::RecordingTransport;
use afr_sandbox::{Engine, Limits, SandboxRequest};
use afr_tools::catalog::SHELL;
use afr_tools::{Catalog, Lease, ToolContext, ToolErrorCode, ToolOutput};
use libtest_mimic::Failed;
use serde_json::{Value, json};

use crate::lane::Lane;
use crate::run::{REACH_OUT, UNREACHABLE, expect, runtime};

/// A command that prints and then fails.
const EXIT_THREE: &str = "echo hi; exit 3";
/// The status that command fails with.
const THREE: i32 = 3;
/// Two sleeps in one process group, the shell waiting on the second.
const SLEEP_TWICE: &str = "sleep 60 & sleep 60";
/// How long the sleeping command is given before its group is killed.
const SHORT_TIMEOUT_MS: u64 = 500;
/// Prints how many `sleep` processes still live, zombies aside, giving a
/// killed group two seconds to go.
const SLEEPERS_LEFT: &str = "for _ in 1 2 3 4 5 6 7 8 9 10; do \
     n=$(cat /proc/[0-9]*/stat 2>/dev/null | grep -c '(sleep) [^Z]'); \
     [ \"$n\" = 0 ] && break; sleep 0.2; done; echo \"$n\"";
/// What that count reads when the whole group is gone.
const NONE_LEFT: &str = "0";
/// Prints the status of a process the shell started, capabilities included.
const OWN_STATUS: &str = "cat /proc/self/status";

/// `outputs` as exactly `N` answers.
pub(crate) fn answers<const N: usize>(outputs: &[ToolOutput]) -> Result<&[ToolOutput; N], Failed> {
    outputs
        .try_into()
        .map_err(|_wrong_count| format!("{N} answers, got {outputs:?}").into())
}

/// `shell`'s arguments: `command`, with `timeout_ms` when one is given.
pub(crate) fn shell(command: &str, timeout_ms: Option<u64>) -> Value {
    json!({"command": command, "timeout_ms": timeout_ms})
}

/// Makes `calls` to `shell`, one after another, in one fresh sandbox, through
/// the handler the runner's catalog hosts; what each answered, in order.
pub(crate) fn shell_calls(
    lane: &Lane,
    lease_id: &str,
    calls: &[Value],
) -> Result<Vec<ToolOutput>, Failed> {
    runtime().block_on(async {
        let (transport, _sent) = RecordingTransport::replying(200, "");
        let catalog = Catalog::hosted(Arc::new(transport));
        let selection = catalog.select(&[SHELL.name()])?;
        let tool = selection
            .tool(SHELL.name())
            .ok_or("the runner hosts shell")?;
        let engine = lane.engine();
        let request = SandboxRequest {
            lease_id,
            limits: Limits::default(),
        };
        let sandbox = engine.prepare(request).await?;
        let lease = Lease::default();
        let mut outputs = Vec::with_capacity(calls.len());
        for arguments in calls {
            let context = ToolContext {
                executor: Some(sandbox.executor()),
                lease: &lease,
            };
            outputs.push(tool.call(arguments, context).await);
        }
        sandbox.destroy().await?;
        Ok(outputs)
    })
}

/// `shell` runs inside the sandbox, and the call carries the exit status and
/// leads with the output the thread's cell shows.
pub(crate) fn shell_exit_code(lane: &Lane) -> Result<(), Failed> {
    let outputs = shell_calls(lane, "shell-exit", &[shell(EXIT_THREE, None)])?;
    let [ran] = answers(&outputs)?;
    expect(
        ran.exit_code == Some(THREE) && ran.error_code.is_none(),
        format!("exit status {THREE} and no code, got {ran:?}"),
    )?;
    expect(
        ran.text.starts_with("hi\n"),
        format!("the output leads, got {:?}", ran.text),
    )
}

/// A command past its timeout is killed with every process in its group.
pub(crate) fn shell_timeout(lane: &Lane) -> Result<(), Failed> {
    let calls = [
        shell(SLEEP_TWICE, Some(SHORT_TIMEOUT_MS)),
        shell(SLEEPERS_LEFT, None),
    ];
    let outputs = shell_calls(lane, "shell-timeout", &calls)?;
    let [timed, left] = answers(&outputs)?;
    expect(
        timed.error_code == Some(ToolErrorCode::TimedOut),
        format!("timed_out, got {timed:?}"),
    )?;
    expect(
        left.text.trim() == NONE_LEFT,
        format!("no sleep outlives its group, got {:?}", left.text),
    )
}

/// A process `shell` starts holds no capability and cannot leave loopback.
pub(crate) fn shell_inherits_sandbox(lane: &Lane) -> Result<(), Failed> {
    let calls = [shell(OWN_STATUS, None), shell(REACH_OUT, None)];
    let outputs = shell_calls(lane, "shell-confined", &calls)?;
    let [status, reach] = answers(&outputs)?;
    afr_sandbox::capabilities_dropped(&status.text)?;
    expect(
        reach.text.contains(UNREACHABLE),
        format!("no route out, got {:?}", reach.text),
    )
}
