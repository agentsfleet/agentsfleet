//! The tenant leaf's freeze and thaw: through the freezer the guest handed
//! in, a failed freeze thawed before it is answered, and refused outright
//! where there is no leaf.

#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs::{self, File, OpenOptions};
use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use super::{Inherit, LeafFreezer, Placement, Tenant};

/// A freezer standing in for the guest's: it records each request in order
/// and fails the freezes it is told to.
#[derive(Debug, Default)]
pub(crate) struct Recorded {
    asked: Mutex<Vec<&'static str>>,
    refuses_freeze: bool,
}

impl Recorded {
    /// One that freezes and thaws, shared as a tenant holds it.
    pub(crate) fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// One whose every freeze fails, as a leaf that never settles does.
    fn refusing() -> Arc<Self> {
        Arc::new(Self {
            refuses_freeze: true,
            ..Self::default()
        })
    }

    /// Each request it was asked, in order.
    pub(crate) fn asked(&self) -> Vec<&'static str> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn ask(&self, request: &'static str) {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request);
    }
}

impl LeafFreezer for Recorded {
    fn freeze(&self) -> io::Result<()> {
        self.ask("freeze");
        if self.refuses_freeze {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        Ok(())
    }

    fn thaw(&self) -> io::Result<()> {
        self.ask("thaw");
        Ok(())
    }
}

/// A tenant over plain files standing in for the leaf's, frozen by
/// `freezer`; its `cgroup.procs` is in the directory answered beside it.
pub(crate) fn tenant(freezer: Arc<Recorded>) -> (tempfile::TempDir, Tenant) {
    let dir = tempfile::tempdir().unwrap();
    let procs = dir.path().join("cgroup.procs");
    let events = dir.path().join("memory.events");
    let pids_events = dir.path().join("pids.events");
    fs::write(&procs, "").unwrap();
    fs::write(&events, "oom_kill 0\n").unwrap();
    fs::write(&pids_events, "max 0\n").unwrap();
    let procs = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&procs)
        .unwrap();
    let tenant = Tenant::new(
        procs.into(),
        File::open(&events).unwrap().into(),
        File::open(&pids_events).unwrap().into(),
        freezer,
    )
    .unwrap();
    (dir, tenant)
}

#[test]
fn a_tenant_freezes_and_thaws_its_leaf_through_the_freezer_it_was_given() {
    let freezer = Recorded::shared();
    let (_dir, tenant) = tenant(Arc::clone(&freezer));

    tenant.freeze().unwrap();
    tenant.thaw().unwrap();

    assert_eq!(freezer.asked(), ["freeze", "thaw"]);
}

/// A freeze that did not settle may hold part of the leaf, so it is thawed
/// before the refusal goes back: a failed pause holds no tenant process.
#[test]
fn a_failed_freeze_is_thawed_before_it_is_refused() {
    let freezer = Recorded::refusing();
    let (_dir, tenant) = tenant(Arc::clone(&freezer));

    let refused = tenant.freeze().unwrap_err();

    assert_eq!(freezer.asked(), ["freeze", "thaw"]);
    assert!(!refused.is_no_tenant_leaf());
    assert!(
        refused
            .wire_message()
            .starts_with("the tenant leaf could not be frozen"),
        "{}",
        refused.wire_message()
    );
}

/// With no leaf there is nothing to freeze, and the refusal says so, apart
/// from a freeze that failed.
#[test]
fn an_agent_with_no_leaf_refuses_to_freeze_or_thaw() {
    let frozen = Inherit.freeze().unwrap_err();
    let thawed = Inherit.thaw().unwrap_err();

    assert!(frozen.is_no_tenant_leaf());
    assert!(thawed.is_no_tenant_leaf());
}
