#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::fs;
use std::os::fd::{IntoRawFd as _, RawFd};
use std::path::Path;

use rustix::io::FdFlags;

use super::{TenantDescriptors, TenantFiles};
use crate::cgroup::{CGROUP_PROCS, MEMORY_EVENTS};

/// `memory.events` as a kernel renders it for a leaf with no kills yet.
const EVENTS: &str = "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\noom_group_kill 0\n";
/// A number no test process holds a descriptor under.
const NEVER_OPENED: RawFd = 1_000_000;

/// The tenant leaf's two files, opened in a plain directory standing in for
/// the leaf.
fn opened(leaf: &Path) -> TenantFiles {
    TenantFiles::open(&leaf.join("cgroup.procs"), &leaf.join("memory.events")).unwrap()
}

#[test]
fn the_engine_opens_both_files_close_on_exec_until_it_lets_them_through() {
    let leaf = tempfile::tempdir().unwrap();

    let files = opened(leaf.path());
    let named = files.descriptors();

    assert_ne!(named.tenant_procs, named.tenant_events);
    let flags = || [&files.procs, &files.events].map(|held| rustix::io::fcntl_getfd(held).unwrap());
    assert!(flags().iter().all(|f| f.contains(FdFlags::CLOEXEC)));
    files.inherit().unwrap();
    assert!(
        flags().iter().all(|f| !f.contains(FdFlags::CLOEXEC)),
        "the next exec keeps both"
    );
}

#[test]
fn the_entry_adopts_what_the_engine_handed_it() {
    let leaf = tempfile::tempdir().unwrap();
    fs::write(leaf.path().join("memory.events"), EVENTS).unwrap();
    let TenantFiles { procs, events } = opened(leaf.path());
    // What an exec does: the numbers stay, and nothing here owns them.
    let named = TenantDescriptors {
        tenant_procs: procs.into_raw_fd(),
        tenant_events: events.into_raw_fd(),
    };

    named.adopt().unwrap();
}

#[test]
fn the_entry_refuses_a_number_it_does_not_hold_or_holds_twice() {
    for (named, refused) in [
        (
            TenantDescriptors {
                tenant_procs: 1,
                tenant_events: 4,
            },
            "descriptor 1 ",
        ),
        (
            TenantDescriptors {
                tenant_procs: 5,
                tenant_events: 5,
            },
            "descriptor 5 ",
        ),
        (
            TenantDescriptors {
                tenant_procs: 2,
                tenant_events: NEVER_OPENED,
            },
            "descriptor 2 ",
        ),
    ] {
        let said = named.adopt().unwrap_err().to_string();

        assert!(said.contains(refused), "{named:?}: {said}");
        assert!(said.contains("not inherited"), "{said}");
    }
}

#[test]
fn a_number_never_opened_is_refused() {
    let leaf = tempfile::tempdir().unwrap();
    let TenantFiles { procs, .. } = opened(leaf.path());
    let named = TenantDescriptors {
        tenant_procs: procs.into_raw_fd(),
        tenant_events: NEVER_OPENED,
    };

    let said = named.adopt().unwrap_err().to_string();

    assert!(said.contains(&NEVER_OPENED.to_string()), "{said}");
}

/// A leaf file the engine cannot open refuses the lease naming that file, so
/// the operator reads which of the two the kernel would not give.
#[test]
fn a_leaf_file_the_engine_cannot_open_is_named() {
    let leaf = tempfile::tempdir().unwrap();
    let absent = leaf.path().join("absent");
    let (procs, events) = (
        leaf.path().join(CGROUP_PROCS),
        leaf.path().join(MEMORY_EVENTS),
    );

    for (procs, events, named) in [
        (absent.join(CGROUP_PROCS), events.clone(), CGROUP_PROCS),
        (procs, absent.join(MEMORY_EVENTS), MEMORY_EVENTS),
    ] {
        let said = TenantFiles::open(&procs, &events).unwrap_err().to_string();

        assert!(said.contains(named), "{named}: {said}");
    }
}
