//! The executor's resident leaf never proves the tenant tree empty.

use std::fs;

use super::LeaseCgroup;
use crate::engine::Limits;

#[test]
fn recovery_quiescence_reads_the_entire_tenant_subtree() -> crate::Result<()> {
    let root = tempfile::tempdir()?;
    let group = LeaseCgroup::create(root.path(), "recovery-boundary", &Limits::default())?;
    // Its parent may contain the resident executor while tenant custody is empty.
    fs::write(
        root.path().join("recovery-boundary/cgroup.events"),
        "populated 1\nfrozen 1\n",
    )?;
    fs::write(group.tenant_events(), "populated 0\n")?;
    assert!(
        group.tenant_empty().is_err(),
        "missing kernel proof fails closed"
    );
    for (events, expected) in [
        ("populated 0\nfrozen 1\n", true),
        ("populated 1\nfrozen 1\n", false),
        ("frozen 1\n", false),
        ("populated 0\npopulated 1\n", false),
    ] {
        fs::write(
            root.path().join("recovery-boundary/tenant/cgroup.events"),
            events,
        )?;
        assert_eq!(group.tenant_empty()?, expected, "{events}");
    }
    Ok(())
}
