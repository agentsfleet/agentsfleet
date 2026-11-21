//! A listing whose directory is read but one of whose entries cannot be:
//! the listing fails in the system's words rather than leaving it out.

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use rustix::fs::{Mode, OFlags};

use super::LocalEnv;
use crate::tools::env::{Env as _, EnvErrorKind};

/// The longest path the host's file calls take.
#[cfg(target_os = "macos")]
const PATH_MAX: usize = 1024;
#[cfg(not(target_os = "macos"))]
const PATH_MAX: usize = 4096;

/// How long each directory's name is: well under a name's limit.
const NAME: usize = 200;

/// The longest name one path component takes.
const NAME_MAX: usize = 255;

/// An entry whose own path reaches [`PATH_MAX`] while its directory's path
/// stays under it is listed, but cannot be looked at: the listing fails,
/// naming why.
#[tokio::test]
async fn a_listing_fails_on_an_entry_it_cannot_look_at() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let env = LocalEnv::new(root.path()).expect("the directory exists");
    let base = std::path::PathBuf::from(env.root());
    let mut relative = Vec::new();
    // Deep enough that one name of at most NAME_MAX bytes takes the entry's
    // path to PATH_MAX, which counts the terminating zero.
    while base.join(relative.join("/")).as_os_str().len() + 1 + NAME_MAX < PATH_MAX {
        relative.push("d".repeat(NAME));
    }
    let deep = relative.join("/");
    let directory_length = base.join(&deep).as_os_str().len();
    let name = "e".repeat(PATH_MAX - directory_length - 1);
    std::fs::create_dir_all(base.join(&deep)).expect("the deep directory is made");
    let directory = rustix::fs::open(
        base.join(&deep).as_path(),
        OFlags::RDONLY | OFlags::DIRECTORY,
        Mode::empty(),
    )
    .expect("the deep directory opens");
    // Made beside the directory, so its own path never has to be spelled.
    let _entry = rustix::fs::openat(
        &directory,
        name.as_str(),
        OFlags::CREATE | OFlags::WRONLY,
        Mode::from_raw_mode(0o644),
    )
    .expect("the entry is made");

    let listed = env.list_dir(&deep).await.err();

    assert_eq!(
        listed.map(|failure| failure.kind()),
        Some(EnvErrorKind::Unavailable)
    );
}
