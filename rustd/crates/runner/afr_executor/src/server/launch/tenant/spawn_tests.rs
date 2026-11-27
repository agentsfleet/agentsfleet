//! Placement at spawn: a process writes itself into the leaf before it
//! runs, a move refused before the fork or in the child refuses the spawn,
//! and with no leaf a process stays where the executor runs.

#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::mpsc;

use super::freeze_tests::{Recorded, tenant as recorded_tenant};
use super::{Inherit, Placement, SIGKILL};
use crate::api::Ending;
use crate::error::{Result, tenant_unavailable};
use crate::protocol::SpawnParams;
use crate::server::files::Workspace;
use crate::server::launch::{Plan, launcher};

/// A placement that refuses where it is told to, standing in for a lost
/// descriptor and for a kernel that refuses the move.
#[derive(Debug)]
enum Refusing {
    BeforeTheFork,
    InTheChild,
}

impl Placement for Refusing {
    fn check(&self) -> Result<()> {
        match self {
            Self::BeforeTheFork => Err(tenant_unavailable(io::Error::from(
                io::ErrorKind::PermissionDenied,
            ))),
            Self::InTheChild => Ok(()),
        }
    }

    fn enter(&self) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::PermissionDenied))
    }

    fn judge(&self, ending: Ending) -> Ending {
        ending
    }

    fn idle(&self) -> io::Result<bool> {
        Ok(true)
    }

    fn pids_refused(&self) -> io::Result<u64> {
        Ok(0)
    }
}

/// A plan that would leave `marker` behind in `root` if it ever ran.
fn marking(root: &Path, terminal: bool) -> (Plan, Workspace) {
    let workspace = Workspace::open(root).unwrap();
    let params = SpawnParams {
        argv: Cow::Owned(vec!["touch".to_owned(), "marker".to_owned()]),
        cwd: None,
        env: Cow::Owned(BTreeMap::new()),
        pty: terminal,
        timeout_ms: None,
    };
    (Plan::new(params, &workspace).unwrap(), workspace)
}

#[test]
fn a_process_writes_itself_into_the_leaf_before_it_runs() {
    let (leaf, tenant) = recorded_tenant(Recorded::shared());
    let root = tempfile::tempdir().unwrap();
    let (plan, _workspace) = marking(root.path(), false);
    let placement: Arc<dyn Placement> = Arc::new(tenant);

    let spawned = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (_input, queued) = mpsc::channel(1);
            let spawned = launcher(false).launch(&plan, &placement, queued).unwrap();
            spawned.exit.await
        });

    assert_eq!(spawned, Ending::Exited(0));
    let moved = fs::read_to_string(leaf.path().join("cgroup.procs")).unwrap();
    assert_eq!(moved, "0", "the child wrote itself, as `0`");
    assert!(root.path().join("marker").exists());
}

#[test]
fn test_failed_tenant_move_refuses_the_spawn() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for (refusing, terminal) in [
        (Refusing::BeforeTheFork, false),
        (Refusing::BeforeTheFork, true),
        (Refusing::InTheChild, false),
        (Refusing::InTheChild, true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let (plan, _workspace) = marking(root.path(), terminal);
        let placement: Arc<dyn Placement> = Arc::new(refusing);

        let refused = runtime.block_on(async {
            let (_input, queued) = mpsc::channel(1);
            launcher(terminal)
                .launch(&plan, &placement, queued)
                .map(drop)
        });

        assert!(refused.is_err(), "{placement:?} on terminal={terminal}");
        assert!(
            !root.path().join("marker").exists(),
            "{placement:?} on terminal={terminal}: nothing ran unplaced"
        );
    }
}

#[test]
fn processes_stay_where_the_executor_runs_without_a_leaf() {
    Inherit.check().unwrap();
    Inherit.enter().unwrap();
    assert_eq!(
        Inherit.judge(Ending::Signaled(SIGKILL)),
        Ending::Signaled(SIGKILL)
    );
}
