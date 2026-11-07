//! A lease whose workspace holds a checkout runs `git` in it, without the
//! model having to find it first.

use afd_wire::policy::{RepositoryAccess, RepositoryBinding};
use afr_tools::catalog::GIT;
use afr_tools::sandbox::{ScriptedExecutor, ScriptedProcess};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::session_tests::drive_in;
use crate::fixture::{Script, call, lease, say, unbounded};

/// The repository the lease binds, and the directory it is checked out in.
const REPOSITORY: &str = "acme/widget";
const CHECKOUT: &str = "widget";

#[tokio::test(start_paused = true)]
async fn git_runs_in_the_repository_the_supervisor_checked_out() {
    let mut bound = lease(&[GIT.name()], unbounded());
    bound.policy.repository_binding = Some(RepositoryBinding {
        repositories: vec![REPOSITORY.into()],
        access: RepositoryAccess::Read,
        base_branch: "main".into(),
    });
    let script = Script::new([
        vec![call("c1", GIT.name(), json!({"args": ["status"]}))],
        vec![say("nothing to commit")],
    ]);
    let executor = ScriptedExecutor::new([ScriptedProcess::exits("clean\n", 0)]);

    drive_in(&script, &bound, &executor, &CancellationToken::new()).await;

    let directories: Vec<_> = executor
        .spawned()
        .iter()
        .map(|spawn| spawn.working_directory().map(str::to_owned))
        .collect();
    assert_eq!(directories, [Some(CHECKOUT.to_owned())]);
}
