//! No restore/capture operation may follow a tenant-controlled link outside root.

use std::{fs, os::unix::{fs::{symlink, MetadataExt as _, PermissionsExt as _}, net::UnixListener}};
use super::files::{Entry, Workspace};

#[test]
fn changed_capture_identity_and_link_replacement_refuse_outside_bytes() -> crate::Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    fs::write(outside.path().join("secret"), b"outside private data")?;
    fs::create_dir(root.path().join("repo"))?;
    fs::write(root.path().join("repo/file"), b"first")?;
    let workspace = Workspace::open(root.path())?;
    let stop = tokio_util::sync::CancellationToken::new();
    let inventory = workspace.inventory(&stop)?;
    let Some(Entry::File { identity, .. }) = inventory.get("repo/file") else {
        return Err(crate::error::config("fixture inventory lost its regular file"));
    };
    assert_eq!(workspace.chunk("repo/file", identity, 0)?.as_slice(), b"first");
    fs::write(root.path().join("repo/file"), b"changed")?;
    assert!(workspace.chunk("repo/file", identity, 0).is_err());
    fs::remove_file(root.path().join("repo/file"))?;
    symlink(outside.path().join("secret"), root.path().join("repo/file"))?;
    assert!(workspace.chunk("repo/file", identity, 0).is_err());
    assert!(workspace.inventory(&stop).is_err());
    fs::remove_file(root.path().join("repo/file"))?;
    fs::remove_dir(root.path().join("repo"))?;
    symlink(outside.path(), root.path().join("repo"))?;
    assert!(workspace.chunk("repo/file", identity, 0).is_err());
    assert_eq!(fs::read(outside.path().join("secret"))?, b"outside private data");
    Ok(())
}

#[test]
fn restore_is_exclusive_bounded_and_preserves_owner_only_modes() -> crate::Result<()> {
    let root = tempfile::tempdir()?;
    let metadata = root.path().metadata()?;
    let owner = (metadata.uid(), metadata.gid());
    let workspace = Workspace::open(root.path())?;
    workspace.require_empty()?;
    workspace.make_directory("repo", owner)?;
    let mut file = workspace.make_file("repo/file", 4, true, owner)?;
    assert!(file.append(b"overflow").is_err());
    file.append(b"data")?;
    file.finish()?;
    assert_eq!(fs::read(root.path().join("repo/file"))?, b"data");
    assert_eq!(fs::metadata(root.path().join("repo/file"))?.permissions().mode() & 0o777, 0o700);
    assert!(workspace.make_file("repo/file", 0, false, owner).is_err());
    assert!(workspace.make_file("../escape", 0, false, owner).is_err());
    assert!(workspace.make_link("repo/escape", "../../outside", owner).is_err());
    workspace.make_link("repo/link", "file", owner)?;
    assert!(workspace.make_file("repo/link", 0, false, owner).is_err());
    assert!(workspace.make_file("repo/short", 1, false, owner)?.finish().is_err());
    assert!(workspace.require_empty().is_err());
    Ok(())
}

#[test]
fn special_files_and_cancelled_inventory_are_refused_without_opening_them() -> crate::Result<()> {
    let root = tempfile::tempdir()?;
    let workspace = Workspace::open(root.path())?;
    let listener = UnixListener::bind(root.path().join("socket"))?;
    assert!(workspace.inventory(&tokio_util::sync::CancellationToken::new()).is_err());
    drop(listener);
    fs::remove_file(root.path().join("socket"))?;
    fs::write(root.path().join("file"), b"data")?;
    let stop = tokio_util::sync::CancellationToken::new();
    stop.cancel();
    assert!(workspace.inventory(&stop).is_err());
    Ok(())
}

#[test]
fn individually_relative_links_cannot_combine_into_a_root_escape() -> crate::Result<()> {
    // Both targets pass a lexical check alone. Following a -> . before the
    // second parent traversal would nevertheless leave the workspace.
    assert!(super::links::validate([("a", "."), ("x/link", "../a/../outside")].into_iter()).is_err());
    assert!(super::links::validate([("a", "b"), ("b", "a")].into_iter()).is_err());
    super::links::validate([("x/link", "../file"), ("alias", "x/link")].into_iter())?;
    Ok(())
}
