//! The supervisor's own checkout for the conformance suite: the runner's
//! `workspace_clone` fetches a repository on the host and imports it into the
//! sandbox, owned by the sandbox's user.

use std::fs;
use std::path::Path;
use std::process::Command;

use afr_engine::Sandbox;
use afr_supervisor::staging::Staging;
use afr_supervisor::workspace_clone::{Mirrors, Request};
use afr_tools::sandbox::Checkout;
use libtest_mimic::Failed;
use tokio_util::sync::CancellationToken;

use crate::run::expect;

/// The repository checked out, its parts, and the branch it starts on.
const REPOSITORY: &str = "acme/widget";
const OWNER: &str = "acme";
const NAME: &str = "widget";
const BASE: &str = "main";
/// The workspace the run's fleet belongs to, which keys its mirror.
const SCOPE: &str = "01890a5d-ac96-774b-bcce-b302099a805a";
/// Where the host serves repositories from, and keeps their mirrors.
const ORIGINS: &str = "origins";
const MIRRORS: &str = "mirrors";
/// Where the checkout waits on the host before it is imported.
const STAGING: &str = "staging";
/// The line the fixture's first commit writes.
const FIRST: &str = "first";
/// The file the fixture commits.
const README: &str = "README.md";
/// Who the fixture's commit names, author and committer alike.
const FIXTURE_NAME: &str = "fixture";
const FIXTURE_EMAIL: &str = "fixture@example.com";
/// The `git` words the fixture spells.
const QUIET: &str = "--quiet";
const MESSAGE: &str = "--message";
const COMMIT: &str = "commit";

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
