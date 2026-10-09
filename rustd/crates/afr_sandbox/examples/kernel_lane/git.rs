//! `git` on a checkout the supervisor made: the runner's own
//! `workspace_clone` fetches a repository on the host and hands it to the
//! sandbox's user, and the `git` handler works it from inside the sandbox.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use afr_egress::testing::RecordingTransport;
use afr_sandbox::{Engine, Limits, Sandbox, SandboxRequest};
use afr_supervisor::workspace_clone::{Mirrors, Request};
use afr_tools::catalog::{GIT, SHELL};
use afr_tools::sandbox::Checkout;
use afr_tools::{Catalog, Lease, Selection, ToolContext, ToolOutput};
use libtest_mimic::Failed;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::lane::Lane;
use crate::run::{expect, runtime};

/// The repository checked out, its parts, and the branch it starts on.
const REPOSITORY: &str = "acme/widget";
const OWNER: &str = "acme";
const NAME: &str = "widget";
const BASE: &str = "main";
/// The workspace the lease's fleet belongs to, which keys its mirror.
const SCOPE: &str = "01890a5d-ac96-774b-bcce-b302099a805a";
/// A token shaped like an installation token; the fetch presents it, and
/// nothing inside the sandbox may hold it.
const TOKEN: &str = "ghs_kernelLaneTokenNeverInside";
/// Where the host serves repositories from, and keeps their mirrors.
const ORIGINS: &str = "origins";
const MIRRORS: &str = "mirrors";
/// The line the fixture's first commit writes, and the one a trial appends.
const FIRST: &str = "first";
const APPENDED: &str = "appended";
/// The directory `mkfs` leaves on the workspace disk.
const LOST_AND_FOUND: &str = "lost+found";
/// The file the fixture commits and a trial edits.
const README: &str = "README.md";
/// Who the fixture's commit names, author and committer alike.
const FIXTURE_NAME: &str = "fixture";
const FIXTURE_EMAIL: &str = "fixture@example.com";
/// The `git` words more than one call spells.
const QUIET: &str = "--quiet";
const MESSAGE: &str = "--message";
const COMMIT: &str = "commit";
const LOG: &str = "log";
const STATUS: &str = "status";

/// Runs `git` on the host, reading no configuration of the host's own.
fn host_git(dir: &Path, args: &[&str]) -> Result<(), Failed> {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", FIXTURE_NAME)
        .env("GIT_AUTHOR_EMAIL", FIXTURE_EMAIL)
        .env("GIT_COMMITTER_NAME", FIXTURE_NAME)
        .env("GIT_COMMITTER_EMAIL", FIXTURE_EMAIL)
        .status()?;
    expect(status.success(), format!("host git {args:?}: {status}"))
}

/// [`REPOSITORY`] with one commit, served from under `root`.
fn serve(root: &Path) -> Result<(), Failed> {
    let remote = root.join(ORIGINS).join(OWNER).join(format!("{NAME}.git"));
    fs::create_dir_all(&remote)?;
    host_git(&remote, &["init", QUIET, "--initial-branch", BASE])?;
    fs::write(remote.join(README), format!("{FIRST}\n"))?;
    host_git(&remote, &["add", README])?;
    host_git(&remote, &[COMMIT, QUIET, MESSAGE, FIRST])
}

/// `git`'s arguments.
fn git(args: &[&str]) -> (&'static str, Value) {
    (GIT.name(), json!({ "args": args }))
}

/// `shell`'s arguments.
fn shell(command: &str) -> (&'static str, Value) {
    (SHELL.name(), json!({ "command": command }))
}

/// Checks [`REPOSITORY`] out into a fresh sandbox through the supervisor's
/// clone, then makes `calls` in order; what each answered.
fn checked_out(
    lane: &Lane,
    lease_id: &str,
    calls: &[(&str, Value)],
) -> Result<Vec<ToolOutput>, Failed> {
    let root = tempfile::tempdir()?;
    serve(root.path())?;
    runtime().block_on(async {
        let (transport, _sent) = RecordingTransport::replying(200, "");
        let catalog = Catalog::hosted(Arc::new(transport));
        let selection = catalog.select(&[GIT.name(), SHELL.name()])?;
        let engine = lane.engine();
        let request = SandboxRequest::new(lease_id, Limits::default());
        let sandbox = engine.prepare(request).await?;
        let outputs = work(root.path(), &selection, sandbox.as_ref(), calls).await;
        sandbox.destroy().await?;
        outputs
    })
}

/// The checkout and the calls, in a sandbox already built.
async fn work(
    root: &Path,
    selection: &Selection<'_>,
    sandbox: &dyn Sandbox,
    calls: &[(&str, Value)],
) -> Result<Vec<ToolOutput>, Failed> {
    let workspace = sandbox
        .workspace()
        .ok_or("bubblewrap offers the host its workspace")?;
    let checkout = Checkout {
        repository: REPOSITORY,
        owner: OWNER,
        name: NAME,
        base: BASE,
    };
    let mirrors = Mirrors::new(
        root.join(MIRRORS),
        format!("file://{}/", root.join(ORIGINS).display()),
    );
    let request = Request {
        scope: SCOPE,
        checkout,
        token: TOKEN,
        workspace,
    };
    mirrors
        .check_out(request, &CancellationToken::new())
        .await?;
    let lease = Lease::default().with_checkouts(vec![checkout]);
    let mut outputs = Vec::with_capacity(calls.len());
    for (name, arguments) in calls {
        let tool = selection.tool(name).ok_or("the runner hosts it")?;
        let context = ToolContext {
            executor: Some(sandbox.executor()),
            lease: &lease,
        };
        outputs.push(tool.call(arguments, context).await);
    }
    Ok(outputs)
}

/// Every answer exited 0 with no code.
fn all_succeeded(outputs: &[ToolOutput]) -> Result<(), Failed> {
    for output in outputs {
        expect(
            output.exit_code == Some(0) && output.error_code.is_none(),
            format!("exit 0 and no code, got {output:?}"),
        )?;
    }
    Ok(())
}

/// `status`, `log`, `diff`, `checkout -b` and `commit` work the supervisor's
/// checkout from inside the sandbox, as the sandbox's user.
pub(crate) fn git_runs_local_commands(lane: &Lane) -> Result<(), Failed> {
    let calls = [
        git(&[STATUS]),
        git(&[LOG, "--format=%s"]),
        shell(&format!("echo {APPENDED} >> {NAME}/README.md")),
        git(&["diff"]),
        git(&["checkout", "-b", "x"]),
        git(&[COMMIT, "--all", MESSAGE, APPENDED]),
        git(&[LOG, "-1", "--format=%an %s"]),
    ];
    let outputs = checked_out(lane, "git-local", &calls)?;
    all_succeeded(&outputs)?;
    let [status, log, _write, diff, _branch, _commit, last] = outputs.as_slice() else {
        return Err(format!("{} answers, got {outputs:?}", calls.len()).into());
    };
    expect(
        status.text.contains("On branch main"),
        format!("the base branch is checked out, got {:?}", status.text),
    )?;
    expect(
        log.text.trim() == FIRST,
        format!("the base's history, got {:?}", log.text),
    )?;
    expect(
        diff.text.contains(&format!("+{APPENDED}")),
        format!("the edit shows, got {:?}", diff.text),
    )?;
    expect(
        last.text.trim() == format!("agentsfleet {APPENDED}"),
        format!("the commit is the sandbox's, got {:?}", last.text),
    )
}

/// The token the supervisor fetched with, raw or as the credentials it
/// presented, is in no file of the workspace and no variable of a process.
/// `lost+found` is the workspace disk's own, root's and unreadable here; any
/// other unreadable file is still reported, and fails the trial.
pub(crate) fn token_never_enters(lane: &Lane) -> Result<(), Failed> {
    let hits = format!(
        "encoded=$(printf 'x-access-token:%s' {TOKEN} | base64 | tr -d '\\n'); \
         (grep -rlF --exclude-dir={LOST_AND_FOUND} -e {TOKEN} -e \"$encoded\" .; env | grep -F -e {TOKEN} -e \"$encoded\") | wc -l"
    );
    let calls = [git(&[STATUS]), shell(&hits)];
    let outputs = checked_out(lane, "git-token", &calls)?;
    all_succeeded(&outputs)?;
    let [_status, found] = outputs.as_slice() else {
        return Err(format!("2 answers, got {outputs:?}").into());
    };
    expect(
        found.text.trim() == "0",
        format!("no file or variable holds the token, got {:?}", found.text),
    )
}
