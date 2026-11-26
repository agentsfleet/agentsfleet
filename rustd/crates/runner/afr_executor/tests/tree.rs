//! A tree crossing the link as a tar stream, both ways, and an import that
//! would land outside the workspace.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::io::Read as _;
use std::path::Path;

use afr_executor::Executor as _;
use sha2::{Digest as _, Sha256};
use tar::{Archive, Builder, EntryType, Header};

use crate::support::{PATIENCE, start};

/// One fixture entry: its name, what it is, its mode, and its link target or
/// content.
type Planted = (&'static str, EntryType, u32, &'static [u8]);

/// A tree with modes kept apart, a relative link and an empty directory.
const FIXTURE: &[Planted] = &[
    (
        "README.md",
        EntryType::Regular,
        0o644,
        b"# morning-rooster\n",
    ),
    ("bin", EntryType::Directory, 0o755, b""),
    (
        "bin/crow",
        EntryType::Regular,
        0o755,
        b"#!/bin/sh\necho crow\n",
    ),
    ("bin/readme", EntryType::Symlink, 0o777, b"../README.md"),
    ("empty", EntryType::Directory, 0o750, b""),
    ("private.key", EntryType::Regular, 0o600, b"not a real key"),
];

/// A tar stream of `entries`, each named as written: a hostile name is
/// planted in the header itself, which the builder would refuse.
fn packed(entries: &[Planted]) -> Vec<u8> {
    let mut builder = Builder::new(Vec::new());
    for &(name, kind, mode, data) in entries {
        let mut header = Header::new_gnu();
        let gnu = header.as_gnu_mut().unwrap();
        gnu.name
            .iter_mut()
            .zip(name.as_bytes())
            .for_each(|(slot, byte)| *slot = *byte);
        header.set_entry_type(kind);
        header.set_mode(mode);
        if kind == EntryType::Symlink {
            header
                .set_link_name(std::str::from_utf8(data).unwrap())
                .unwrap();
            header.set_size(0);
            header.set_cksum();
            builder.append(&header, std::io::empty()).unwrap();
        } else {
            header.set_size(u64::try_from(data.len()).unwrap());
            header.set_cksum();
            builder.append(&header, data).unwrap();
        }
    }
    builder.into_inner().unwrap()
}

/// SHA-256 over a stream's sorted listing: each entry's path, kind, mode,
/// link target and content. A link's own mode means nothing and is left out.
fn digest(stream: &[u8]) -> [u8; 32] {
    let mut listing = Vec::new();
    for entry in Archive::new(stream).entries().unwrap() {
        let mut entry = entry.unwrap();
        let kind = entry.header().entry_type();
        let path = entry
            .path()
            .unwrap()
            .to_string_lossy()
            .trim_end_matches('/')
            .to_owned();
        let mode = if kind == EntryType::Symlink {
            0
        } else {
            entry.header().mode().unwrap() & 0o777
        };
        let target = entry
            .link_name()
            .unwrap()
            .map_or_else(String::new, |target| target.to_string_lossy().into_owned());
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        listing.push((path, kind.as_byte(), mode, target, content));
    }
    listing.sort();
    let mut hasher = Sha256::new();
    for (path, kind, mode, target, content) in listing {
        hasher.update(format!(
            "{path}\0{kind}\0{mode:o}\0{target}\0{}\0",
            content.len()
        ));
        hasher.update(content);
    }
    hasher.finalize().into()
}

/// The names directly in `dir`.
fn names(dir: &Path) -> Vec<String> {
    let mut names = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

// Value: protects=a workspace crosses the link with no host path and comes back the same tree;
// fails_when=a mode, a link, an empty directory or a byte is lost either way;
// seam=real executor and client over a Unix socket; no_claim=ownership or timestamps.
#[tokio::test]
async fn test_import_then_export_returns_the_same_tree() {
    let harness = start().await;
    let fixture = packed(FIXTURE);

    let imported = tokio::time::timeout(
        PATIENCE,
        harness.client.import(".", &mut fixture.as_slice()),
    )
    .await
    .unwrap()
    .expect("the fixture imports");
    let mut exported = Vec::new();
    tokio::time::timeout(PATIENCE, harness.client.export(&["."], &mut exported))
        .await
        .unwrap()
        .expect("the workspace exports");

    assert_eq!(imported.entries, 6);
    assert_eq!(imported.bytes, 52);
    assert_eq!(digest(&exported), digest(&fixture));
    let link = std::fs::read_link(harness.root.join("bin/readme")).unwrap();
    assert_eq!(link, Path::new("../README.md"), "the link stays relative");
}

// Value: protects=an import cannot reach past the workspace by a name or a link's target;
// fails_when=any of the three lands, or an entry before it is written;
// seam=real executor and client over a Unix socket; no_claim=links already in the workspace.
#[tokio::test]
async fn test_an_import_entry_leaving_its_root_is_refused() {
    let harness = start().await;
    let kept: Planted = ("kept.txt", EntryType::Regular, 0o644, b"kept");
    let leaving: [Planted; 3] = [
        ("../x", EntryType::Regular, 0o644, b"planted"),
        ("abs", EntryType::Symlink, 0o777, b"/etc/passwd"),
        ("a/out", EntryType::Symlink, 0o777, b"../../x"),
    ];

    for hostile in leaving {
        let stream = packed(&[kept, hostile]);
        let refused =
            tokio::time::timeout(PATIENCE, harness.client.import(".", &mut stream.as_slice()))
                .await
                .unwrap()
                .expect_err("an entry leaving the workspace refuses the import");

        assert!(refused.is_path_refused(), "{}: {refused}", hostile.0);
        assert_eq!(names(&harness.root), Vec::<String>::new(), "{}", hostile.0);
    }
    assert_eq!(
        names(harness.scratch.path()),
        ["executor.sock", "workspace"]
    );
}
