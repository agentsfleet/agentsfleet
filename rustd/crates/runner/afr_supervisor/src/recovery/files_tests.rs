//! No capture may follow a tenant-controlled link outside root, and no link a
//! restore lands may leave it.

use super::files::{Entry, Identity, Workspace};
use std::{
    fs,
    os::unix::{fs::symlink, net::UnixListener},
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
fn links_that_together_name_the_parent_are_never_followed_by_capture() -> crate::Result<()> {
    // `l -> .` and `x -> l/..` each pass the lexical link check, and together
    // `x` is the workspace's parent. Only opening every directory with no
    // link followed keeps a capture inside.
    let outer = tempfile::tempdir()?;
    let root = outer.path().join("workspace");
    fs::create_dir(&root)?;
    fs::write(outer.path().join("secret"), b"outside private data")?;
    let workspace = Workspace::open(&root)?;
    symlink(".", root.join("l"))?;
    symlink("l/..", root.join("x"))?;
    assert_eq!(fs::read(root.join("x/secret"))?, b"outside private data");
    // The file's own identity: a capture that followed `x` would pass every
    // check after the open and return the bytes.
    let identity = Identity::of(&fs::metadata(outer.path().join("secret"))?);

    let followed = [
        workspace.chunk("x/secret", &identity, 0).is_ok(),
        workspace.directory("x").is_ok(),
    ];

    assert_eq!(followed, [false; 2]);
    let mut outside: Vec<_> = fs::read_dir(outer.path())?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<_>>()?;
    outside.sort();
    assert_eq!(outside, ["secret", "workspace"]);
    Ok(())
}
