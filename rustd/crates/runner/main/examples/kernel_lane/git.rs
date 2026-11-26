//! `git` on a checkout the supervisor made: the runner's own
//! `workspace_clone` fetches a repository on the host and imports it into the
//! sandbox, owned by the sandbox's user, and the `git` handler works it from
//! inside the sandbox.

use std::fs;
use std::path::Path;
use std::process::Command;

use afr_engine::{Engine, Limits, Sandbox};
use afr_supervisor::staging::Staging;
use afr_supervisor::workspace_clone::{Mirrors, Request};
use afr_tools::Run;
use afr_tools::catalog::{GIT, SHELL};
use afr_tools::sandbox::Checkout;
use guffbolt::ToolOutput;
use libtest_mimic::Failed;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::lane::Lane;
use crate::lane::{id_of, spec};
use crate::run::{expect, runtime};
use crate::tools::called;

/// The repository checked out, its parts, and the branch it starts on.
const REPOSITORY: &str = "acme/widget";
const OWNER: &str = "acme";
const NAME: &str = "widget";
const BASE: &str = "main";
/// The workspace the run's fleet belongs to, which keys its mirror.
const SCOPE: &str = "01890a5d-ac96-774b-bcce-b302099a805a";
/// A token shaped like an installation token; the fetch presents it, and
/// nothing inside the sandbox may hold it.
const TOKEN: &str = "ghs_kernelLaneTokenNeverInside";
/// Where the host serves repositories from, and keeps their mirrors.
const ORIGINS: &str = "origins";
const MIRRORS: &str = "mirrors";
/// Where the checkout waits on the host before it is imported.
const STAGING: &str = "staging";
/// The line the fixture's first commit writes, and the one a trial appends.
const FIRST: &str = "first";
const APPENDED: &str = "appended";
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
pub(crate) fn serve(root: &Path) -> Result<(), Failed> {
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

/// The repository the supervisor checks out, as a run names it.
const CHECKOUT: Checkout<'static> = Checkout {
    repository: REPOSITORY,
    owner: OWNER,
    name: NAME,
    base: BASE,
};

/// The supervisor's own checkout of [`REPOSITORY`], served from a directory
/// of its own: what the conformance suite's planted-token search runs after.
pub(crate) struct LaneCheckout {
    root: tempfile::TempDir,
}

impl LaneCheckout {
    /// Serves [`REPOSITORY`] with one commit.
    pub(crate) fn serve() -> Result<Self, Failed> {
        let root = tempfile::tempdir()?;
        serve(root.path())?;
        Ok(Self { root })
    }
}

#[async_trait::async_trait]
impl afr_engine::conformance::Checkout for LaneCheckout {
    /// Fetches into the host mirror with `token`, then imports the tree.
    async fn check_out(&self, sandbox: &dyn Sandbox, token: &str) -> Result<(), Failed> {
        let root = self.root.path();
        let mirrors = Mirrors::new(
            root.join(MIRRORS),
            format!("file://{}/", root.join(ORIGINS).display()),
        );
        let request = Request {
            scope: SCOPE,
            checkout: CHECKOUT,
            token,
        };
        let staged = Staging::sweep(&root.join(STAGING))?.stage(sandbox.id())?;
        mirrors
            .land(
                request,
                &staged,
                sandbox.executor(),
                &CancellationToken::new(),
            )
            .await?;
        Ok(())
    }
}

/// Checks [`REPOSITORY`] out into a fresh sandbox through the supervisor's
/// clone, then makes `calls` in order; what each answered.
fn checked_out(
    lane: &Lane,
    run_id: &str,
    calls: &[(&str, Value)],
) -> Result<Vec<ToolOutput>, Failed> {
    let checkout = LaneCheckout::serve()?;
    runtime().block_on(async {
        let engine = lane.engine();
        let sandbox_id = id_of(run_id);
        let request = spec(&sandbox_id, Limits::default());
        let sandbox = engine.prepare(request).await?;
        let outputs = work(&checkout, sandbox.as_ref(), calls).await;
        sandbox.destroy().await?;
        outputs
    })
}

/// The checkout and the calls, in a sandbox already built.
async fn work(
    checkout: &LaneCheckout,
    sandbox: &dyn Sandbox,
    calls: &[(&str, Value)],
) -> Result<Vec<ToolOutput>, Failed> {
    afr_engine::conformance::Checkout::check_out(checkout, sandbox, TOKEN).await?;
    let run = Run::default().with_checkouts(vec![CHECKOUT]);
    let mut outputs = Vec::with_capacity(calls.len());
    for (step, (name, arguments)) in (1_u64..).zip(calls) {
        if ![GIT.name(), SHELL.name()].contains(name) {
            return Err(format!("{name} is not one of the trial's tools").into());
        }
        outputs.push(called(sandbox.executor(), &run, step, name, arguments).await?);
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
