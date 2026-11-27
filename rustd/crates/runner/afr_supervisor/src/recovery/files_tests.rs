//! No restore/capture operation may follow a tenant-controlled link outside root.

use super::files::{Entry, Identity, Workspace};
use std::{
    fs,
    os::unix::{
        fs::{MetadataExt as _, PermissionsExt as _, symlink},
        net::UnixListener,
    },
};

#[test]
fn link_text_rejects_ascii_and_unicode_controls_without_rejecting_unicode_names()
-> crate::Result<()> {
    for target in [
        "bad\\name",
        "bad\0name",
        "bad\nname",
        "bad\u{7f}name",
        "bad\u{85}name",
        "bad\u{9f}name",
    ] {
        super::files::valid_link("dir/link", target)
            .err()
            .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    }
    super::files::valid_link("dir/link", "../café/日本語")?;
    Ok(())
}

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
        return Err(crate::error::config(
            "fixture inventory lost its regular file",
        ));
    };
    assert_eq!(
        workspace.chunk("repo/file", identity, 0)?.as_slice(),
        b"first"
    );
    fs::write(root.path().join("repo/file"), b"changed")?;
    workspace
        .chunk("repo/file", identity, 0)
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    fs::remove_file(root.path().join("repo/file"))?;
    symlink(outside.path().join("secret"), root.path().join("repo/file"))?;
    workspace
        .chunk("repo/file", identity, 0)
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    workspace
        .inventory(&stop)
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    fs::remove_file(root.path().join("repo/file"))?;
    fs::remove_dir(root.path().join("repo"))?;
    symlink(outside.path(), root.path().join("repo"))?;
    workspace
        .chunk("repo/file", identity, 0)
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    assert_eq!(
        fs::read(outside.path().join("secret"))?,
        b"outside private data"
    );
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
    file.append(b"overflow")
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    file.append(b"data")?;
    file.finish()?;
    assert_eq!(fs::read(root.path().join("repo/file"))?, b"data");
    assert_eq!(
        fs::metadata(root.path().join("repo/file"))?
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    workspace
        .make_file("repo/file", 0, false, owner)
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    workspace
        .make_file("../escape", 0, false, owner)
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    workspace
        .make_link("repo/escape", "../../outside", owner)
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    workspace.make_link("repo/link", "file", owner)?;
    workspace
        .make_file("repo/link", 0, false, owner)
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    workspace
        .make_file("repo/short", 1, false, owner)?
        .finish()
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    workspace
        .require_empty()
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    Ok(())
}

#[test]
fn special_files_and_cancelled_inventory_are_refused_without_opening_them() -> crate::Result<()> {
    let root = tempfile::tempdir()?;
    let workspace = Workspace::open(root.path())?;
    let listener = UnixListener::bind(root.path().join("socket"))?;
    workspace
        .inventory(&tokio_util::sync::CancellationToken::new())
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    drop(listener);
    fs::remove_file(root.path().join("socket"))?;
    fs::write(root.path().join("file"), b"data")?;
    let stop = tokio_util::sync::CancellationToken::new();
    stop.cancel();
    workspace
        .inventory(&stop)
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    Ok(())
}

#[test]
fn individually_relative_links_cannot_combine_into_a_root_escape() -> crate::Result<()> {
    // Both targets pass a lexical check alone. Following a -> . before the
    // second parent traversal would nevertheless leave the workspace.
    super::links::validate([("a", "."), ("x/link", "../a/../outside")].into_iter())
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    super::links::validate([("a", "b"), ("b", "a")].into_iter())
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    super::links::validate([("x/link", "../file"), ("alias", "x/link")].into_iter())?;
    Ok(())
}

#[test]
fn links_that_together_name_the_parent_are_never_followed_by_restore_or_capture()
-> crate::Result<()> {
    // `l -> .` and `x -> l/..` each pass the lexical link check, and together
    // `x` is the workspace's parent. Only opening every directory with no
    // link followed keeps restore and capture inside.
    let outer = tempfile::tempdir()?;
    let root = outer.path().join("workspace");
    fs::create_dir(&root)?;
    fs::write(outer.path().join("secret"), b"outside private data")?;
    let metadata = root.metadata()?;
    let owner = (metadata.uid(), metadata.gid());
    let workspace = Workspace::open(&root)?;
    workspace.make_link("l", ".", owner)?;
    workspace.make_link("x", "l/..", owner)?;
    assert_eq!(fs::read(root.join("x/secret"))?, b"outside private data");
    // The file's own identity: a capture that followed `x` would pass every
    // check after the open and return the bytes.
    let identity = Identity::of(&fs::metadata(outer.path().join("secret"))?);

    let followed = [
        workspace.make_file("x/planted", 0, false, owner).is_ok(),
        workspace.make_directory("x/made", owner).is_ok(),
        workspace.make_link("x/alias", "secret", owner).is_ok(),
        workspace.chunk("x/secret", &identity, 0).is_ok(),
        workspace.directory("x").is_ok(),
    ];

    assert_eq!(followed, [false; 5]);
    let mut outside: Vec<_> = fs::read_dir(outer.path())?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<_>>()?;
    outside.sort();
    assert_eq!(outside, ["secret", "workspace"]);
    assert_eq!(
        fs::read(outer.path().join("secret"))?,
        b"outside private data"
    );
    Ok(())
}
