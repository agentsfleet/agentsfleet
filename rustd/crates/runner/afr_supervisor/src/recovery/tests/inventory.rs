//! A capture inventory refuses links that resolve into a cycle, even though
//! each target is relative and inside the workspace on its own.

use crate::recovery::files::{Entry, Workspace};
use crate::test_support;
use std::os::unix::fs::symlink;
use tokio_util::sync::CancellationToken;

#[test]
fn a_workspace_whose_links_form_a_cycle_is_not_inventoried() -> crate::Result<()> {
    let root = tempfile::tempdir()?;
    symlink("b", root.path().join("a"))?;
    symlink("a", root.path().join("b"))?;
    let workspace = Workspace::open(root.path())?;
    let stop = CancellationToken::new();

    let cyclic = workspace.inventory(&stop);

    assert_eq!(
        test_support::detail(&cyclic),
        Some("checkpoint link graph is invalid")
    );

    // Breaking the cycle with a regular file makes the same link acceptable.
    std::fs::remove_file(root.path().join("b"))?;
    std::fs::write(root.path().join("b"), b"data")?;
    let inventory = workspace.inventory(&stop)?;
    assert!(matches!(
        inventory.get("a"),
        Some(Entry::Symlink { target }) if target == "b"
    ));
    Ok(())
}
