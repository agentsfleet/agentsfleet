#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::OwnedFd;
use std::path::Path;
use std::sync::Arc;

use rustix::io::FdFlags;
use tokio::sync::mpsc;

use super::{Inherit, Placement, SIGKILL, Tenant, oom_kill_count};
use crate::api::Ending;
use crate::error::{Result, tenant_unavailable};
use crate::protocol::SpawnParams;
use crate::server::files::Workspace;
use crate::server::launch::{Plan, launcher};

/// `memory.events` as a kernel renders it, with `kills` processes killed.
fn events(kills: u64) -> String {
    format!("low 0\nhigh 0\nmax 4\noom 2\noom_kill {kills}\noom_group_kill 0\n")
}

/// A stand-in leaf: `cgroup.procs` and `memory.events` as plain files.
struct Leaf {
    dir: tempfile::TempDir,
}

impl Leaf {
    const PROCS: &str = "cgroup.procs";
    const EVENTS: &str = "memory.events";

    fn new(kills: u64) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(Self::PROCS), "").unwrap();
        fs::write(dir.path().join(Self::EVENTS), events(kills)).unwrap();
        Self { dir }
    }

    fn procs(&self) -> OwnedFd {
        let file = OpenOptions::new()
            .write(true)
            .open(self.dir.path().join(Self::PROCS))
            .unwrap();
        file.into()
    }

    fn events(&self) -> OwnedFd {
        File::open(self.dir.path().join(Self::EVENTS))
            .unwrap()
            .into()
    }

    fn kill(&self, kills: u64) {
        fs::write(self.dir.path().join(Self::EVENTS), events(kills)).unwrap();
    }

    fn moved(&self) -> String {
        fs::read_to_string(self.dir.path().join(Self::PROCS)).unwrap()
    }

    fn tenant(&self) -> Tenant {
        Tenant::new(self.procs(), self.events()).unwrap()
    }
}

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
fn the_oom_kill_count_is_read_from_its_own_line() {
    assert_eq!(oom_kill_count(events(3).as_bytes()), Some(3));
    assert_eq!(
        oom_kill_count(b"oom_group_kill 7\n"),
        None,
        "a group kill is another line"
    );
    assert_eq!(oom_kill_count(b"oom_kill many\n"), None);
    assert_eq!(oom_kill_count(&[0xff, 0xfe]), None);
}

#[test]
fn a_tenant_holds_both_descriptors_close_on_exec() {
    let leaf = Leaf::new(0);

    let tenant = leaf.tenant();

    for held in [&tenant.procs, &tenant.events] {
        let flags = rustix::io::fcntl_getfd(held).unwrap();
        assert!(flags.contains(FdFlags::CLOEXEC), "{flags:?}");
    }
}

#[test]
fn a_procs_descriptor_not_open_for_writing_is_refused_with_its_cause() {
    let leaf = Leaf::new(0);
    let read_only = File::open(leaf.dir.path().join(Leaf::PROCS)).unwrap();

    let refused = Tenant::new(read_only.into(), leaf.events()).unwrap_err();

    assert_eq!(refused.wire_message(), {
        let cause = io::Error::from(rustix::io::Errno::BADF);
        format!("the tenant leaf cannot be entered: {cause}")
    });
}

#[test]
fn events_that_do_not_read_as_memory_events_are_refused() {
    let leaf = Leaf::new(0);
    fs::write(leaf.dir.path().join(Leaf::EVENTS), "not events").unwrap();

    let refused = Tenant::new(leaf.procs(), leaf.events());

    assert!(refused.is_err(), "no oom_kill line, no tenant");
}

#[test]
fn a_kill_counted_by_the_leaf_reads_as_out_of_memory_once() {
    let leaf = Leaf::new(2);
    let tenant = leaf.tenant();
    let killed = Ending::Signaled(SIGKILL);

    assert_eq!(tenant.judge(killed), killed, "no new kill, a plain SIGKILL");
    leaf.kill(3);
    assert_eq!(tenant.judge(killed), Ending::OutOfMemory);
    assert_eq!(tenant.judge(killed), killed, "that kill is already told");
}

#[test]
fn only_a_sigkill_is_judged() {
    let leaf = Leaf::new(0);
    let tenant = leaf.tenant();
    leaf.kill(1);

    for ending in [Ending::Exited(137), Ending::Signaled(15), Ending::TimedOut] {
        assert_eq!(tenant.judge(ending), ending);
    }
    assert_eq!(
        tenant.judge(Ending::Signaled(SIGKILL)),
        Ending::OutOfMemory,
        "the kill was still there to be read"
    );
}

#[test]
fn a_process_writes_itself_into_the_leaf_before_it_runs() {
    let leaf = Leaf::new(0);
    let root = tempfile::tempdir().unwrap();
    let (plan, _workspace) = marking(root.path(), false);
    let placement: Arc<dyn Placement> = Arc::new(leaf.tenant());

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
    assert_eq!(leaf.moved(), "0", "the child wrote itself, as `0`");
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
