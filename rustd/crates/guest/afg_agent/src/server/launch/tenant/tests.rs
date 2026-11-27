#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::OwnedFd;

use af_core::test_util::trace::Capture;
use rustix::io::FdFlags;
use tracing::Level;

use super::freeze_tests::Recorded;
use super::{
    EVENT_EVENTS_UNREAD, Inherit, OOM_KILL, PIDS_REFUSED, Placement, SHELL_KILLED_EXIT, SIGKILL,
    Tenant, event_count,
};
use af_guest_wire::Ending;
use af_guest_wire::error::Result;

/// `memory.events` as a kernel renders it, with `kills` processes killed.
fn events(kills: u64) -> String {
    format!("low 0\nhigh 0\nmax 4\noom 2\noom_kill {kills}\noom_group_kill 0\n")
}

/// `pids.events` as a kernel renders it, with `refused` forks refused.
fn pids_events(refused: u64) -> String {
    format!("max {refused}\nmax.imposed 0\n")
}

/// A stand-in leaf: `cgroup.procs`, `memory.events` and `pids.events` as
/// plain files.
struct Leaf {
    dir: tempfile::TempDir,
}

impl Leaf {
    const PROCS: &str = "cgroup.procs";
    const EVENTS: &str = "memory.events";
    const PIDS_EVENTS: &str = "pids.events";

    fn new(kills: u64) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(Self::PROCS), "").unwrap();
        fs::write(dir.path().join(Self::EVENTS), events(kills)).unwrap();
        fs::write(dir.path().join(Self::PIDS_EVENTS), pids_events(0)).unwrap();
        Self { dir }
    }

    fn procs(&self) -> OwnedFd {
        let file = OpenOptions::new()
            .read(true)
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

    fn pids_events(&self) -> OwnedFd {
        File::open(self.dir.path().join(Self::PIDS_EVENTS))
            .unwrap()
            .into()
    }

    fn refuse(&self, refused: u64) {
        fs::write(
            self.dir.path().join(Self::PIDS_EVENTS),
            pids_events(refused),
        )
        .unwrap();
    }

    fn kill(&self, kills: u64) {
        fs::write(self.dir.path().join(Self::EVENTS), events(kills)).unwrap();
    }

    /// A tenant over this leaf's files, its `cgroup.procs` opened as `procs`.
    fn over(&self, procs: OwnedFd) -> Result<Tenant> {
        Tenant::new(procs, self.events(), self.pids_events(), Recorded::shared())
    }

    fn tenant(&self) -> Tenant {
        self.over(self.procs()).unwrap()
    }
}

#[test]
fn the_oom_kill_count_is_read_from_its_own_line() {
    assert_eq!(event_count(events(3).as_bytes(), OOM_KILL), Some(3));
    assert_eq!(
        event_count(b"oom_group_kill 7\n", OOM_KILL),
        None,
        "a group kill is another line"
    );
    assert_eq!(event_count(b"oom_kill many\n", OOM_KILL), None);
    assert_eq!(event_count(&[0xff, 0xfe], OOM_KILL), None);
}

/// The forks refused at the leaf's `pids.max` are its `pids.events` `max`
/// line, never `max.imposed`, which counts an ancestor's cap; with no leaf,
/// nothing is capped and nothing refused.
#[test]
fn the_forks_refused_at_the_process_cap_are_the_leafs_max_count() {
    let leaf = Leaf::new(0);
    let tenant = leaf.tenant();
    let none = tenant.pids_refused().unwrap();

    leaf.refuse(5);

    assert_eq!(none, 0);
    assert_eq!(tenant.pids_refused().unwrap(), 5);
    assert_eq!(
        event_count(b"max.imposed 3\nmax 0\n", PIDS_REFUSED),
        Some(0),
        "an ancestor's refusals are another line"
    );
    assert_eq!(Inherit.pids_refused().unwrap(), 0);
}

#[test]
fn events_that_do_not_read_as_pids_events_are_refused() {
    let leaf = Leaf::new(0);
    fs::write(leaf.dir.path().join(Leaf::PIDS_EVENTS), "not events").unwrap();

    let refused = leaf.over(leaf.procs());

    assert!(refused.is_err(), "no max line, no tenant");
}

#[test]
fn a_tenant_holds_every_descriptor_close_on_exec() {
    let leaf = Leaf::new(0);

    let tenant = leaf.tenant();

    for held in [&tenant.procs, &tenant.events, &tenant.pids_events] {
        let flags = rustix::io::fcntl_getfd(held).unwrap();
        assert!(flags.contains(FdFlags::CLOEXEC), "{flags:?}");
    }
}

#[test]
fn a_procs_descriptor_not_open_for_writing_is_refused_with_its_cause() {
    let leaf = Leaf::new(0);
    let read_only = File::open(leaf.dir.path().join(Leaf::PROCS)).unwrap();

    let refused = leaf.over(read_only.into()).unwrap_err();

    assert_eq!(refused.wire_message(), {
        let cause = io::Error::from(rustix::io::Errno::BADF);
        format!("the tenant leaf cannot be entered: {cause}")
    });
}

/// A leaf is idle only while it lists no process: one a call started and
/// left behind keeps it busy though no call runs. A leaf that cannot be
/// read proves nothing, so it is no tenant at all; with no leaf, nothing is
/// placed and nothing is held.
#[test]
fn a_leaf_is_idle_only_while_it_lists_no_process() {
    let leaf = Leaf::new(0);
    let tenant = leaf.tenant();
    let listed = leaf.dir.path().join(Leaf::PROCS);

    let empty = tenant.idle().unwrap();
    fs::write(&listed, "4242\n").unwrap();
    let holding = tenant.idle().unwrap();
    let unread = OpenOptions::new().write(true).open(&listed).unwrap();
    let refused = leaf.over(unread.into());

    assert!(empty, "no process listed");
    assert!(!holding, "a process a call left behind");
    assert!(refused.is_err(), "a leaf that cannot be listed");
    assert!(Inherit.idle().unwrap());
}

#[test]
fn events_that_do_not_read_as_memory_events_are_refused() {
    let leaf = Leaf::new(0);
    fs::write(leaf.dir.path().join(Leaf::EVENTS), "not events").unwrap();

    let refused = leaf.over(leaf.procs());

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
fn a_kill_whose_count_cannot_be_read_stays_a_sigkill_and_is_logged() {
    let capture = Capture::install();
    let leaf = Leaf::new(0);
    let tenant = leaf.tenant();
    fs::write(leaf.dir.path().join(Leaf::EVENTS), "not events").unwrap();
    let killed = Ending::Signaled(SIGKILL);

    assert_eq!(tenant.judge(killed), killed);

    let unread = capture.only(EVENT_EVENTS_UNREAD);
    assert_eq!(unread.level, Level::WARN);
    assert_eq!(unread.field("error_code"), Some("UZ-INTERNAL-003"));
}

#[test]
fn a_shell_reporting_its_childs_kill_reads_as_out_of_memory_when_the_leaf_counted_it() {
    let leaf = Leaf::new(0);
    let tenant = leaf.tenant();
    leaf.kill(1);

    assert_eq!(
        tenant.judge(Ending::Exited(SHELL_KILLED_EXIT)),
        Ending::OutOfMemory
    );
}

#[test]
fn a_shell_reporting_a_kill_the_leaf_never_counted_keeps_its_exit() {
    let leaf = Leaf::new(2);
    let tenant = leaf.tenant();
    let reported = Ending::Exited(SHELL_KILLED_EXIT);

    assert_eq!(tenant.judge(reported), reported);
}

#[test]
fn only_a_kill_is_judged() {
    let leaf = Leaf::new(0);
    let tenant = leaf.tenant();
    leaf.kill(1);

    for ending in [Ending::Exited(1), Ending::Signaled(15), Ending::TimedOut] {
        assert_eq!(tenant.judge(ending), ending, "the count rose all the same");
    }
    assert_eq!(
        tenant.judge(Ending::Signaled(SIGKILL)),
        Ending::OutOfMemory,
        "the kill was still there to be read"
    );
}
