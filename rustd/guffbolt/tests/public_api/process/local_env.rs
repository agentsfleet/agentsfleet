//! `LocalEnv` on this host: files under its root and the processes it holds.

use super::*;

#[cfg(unix)]
#[tokio::test]
async fn local_env_names_its_root_and_takes_the_defaults_it_does_not_override() {
    let (directory, env) = local();
    let canonical = std::fs::canonicalize(directory.path()).expect("resolves");

    assert_eq!(env.root(), canonical.display().to_string());
    assert_eq!(env.ending_grace(), ENDING_GRACE_DEFAULT);
    assert_eq!(
        env.browser().err(),
        Some("has no browser in this environment")
    );
}

/// Files under the root are written with the directories they need, read to
/// a limit, added to, listed and deleted; a path given absolute under the
/// root reads the same as a relative one.
#[cfg(unix)]
#[tokio::test]
async fn local_env_writes_reads_appends_lists_and_deletes_files_under_its_root() {
    let (directory, env) = local();

    env.write_file("notes/today.txt", Bytes::from_static(b"first"))
        .await
        .expect("written");
    env.append_file("notes/today.txt", Bytes::from_static(b", second"))
        .await
        .expect("added");
    env.append_file("logs/new.txt", Bytes::from_static(b"made"))
        .await
        .expect("made by appending");
    let whole = env.read_file("notes/today.txt", 1024).await.expect("read");
    let absolute = format!("{}/notes/today.txt", env.root());
    let cut = env
        .read_file(&absolute, 5)
        .await
        .expect("read through its absolute path");
    std::fs::create_dir(directory.path().join("notes/inner")).expect("made");
    std::os::unix::fs::symlink("today.txt", directory.path().join("notes/link")).expect("linked");
    let _socket = std::os::unix::net::UnixListener::bind(directory.path().join("notes/sock"))
        .expect("a socket");
    let listing = env.list_dir("notes").await.expect("listed");
    env.delete_file("notes/today.txt").await.expect("deleted");

    assert_eq!(
        (whole.data.as_ref(), whole.truncated),
        (b"first, second".as_slice(), false)
    );
    assert_eq!(
        (cut.data.as_ref(), cut.truncated),
        (b"first".as_slice(), true)
    );
    let mut kinds: Vec<(String, EntryKind)> = listing
        .entries
        .iter()
        .map(|entry| (entry.name.clone(), entry.kind))
        .collect();
    kinds.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        kinds,
        [
            ("inner".to_owned(), EntryKind::Directory),
            ("link".to_owned(), EntryKind::Symlink),
            ("sock".to_owned(), EntryKind::Other),
            ("today.txt".to_owned(), EntryKind::File),
        ]
    );
    let file = listing
        .entries
        .iter()
        .find(|entry| entry.name == "today.txt");
    assert_eq!(file.map(|entry| entry.size), Some(13));
    assert!(!listing.truncated);
    assert!(!directory.path().join("notes/today.txt").exists());
    assert_eq!(
        std::fs::read(directory.path().join("logs/new.txt")).expect("kept"),
        b"made"
    );
}

/// A path that leaves the root, by `..`, by an absolute path elsewhere or
/// through a link, is refused before anything is touched.
#[cfg(unix)]
#[tokio::test]
async fn local_env_refuses_every_path_that_leaves_its_root() {
    let (directory, env) = local();
    let outside = tempfile::tempdir().expect("a second directory");
    std::fs::write(outside.path().join("secret"), b"x").expect("written");
    std::os::unix::fs::symlink(outside.path(), directory.path().join("away")).expect("linked");

    let refusals = [
        env.read_file("../secret", 10).await.err(),
        env.read_file("/etc/hosts", 10).await.err(),
        env.read_file("away/secret", 10).await.err(),
        env.write_file("away/new", Bytes::from_static(b"x"))
            .await
            .err(),
        env.append_file("away/deeper/new", Bytes::from_static(b"x"))
            .await
            .err(),
        env.delete_file("away/secret").await.err(),
        env.list_dir("away").await.err(),
    ];

    let kinds: Vec<Option<(EnvErrorKind, String)>> = refusals
        .iter()
        .map(|refused| {
            refused
                .as_ref()
                .map(|error| (error.kind(), error.message().to_owned()))
        })
        .collect();
    let refused = Some((
        EnvErrorKind::PathRefused,
        "the path leaves the working directory".to_owned(),
    ));
    assert_eq!(kinds, vec![refused; 7]);
    assert!(!outside.path().join("new").exists());
    assert!(outside.path().join("secret").exists());
}

/// Each file call that fails reads the system's own words, sorted: a name
/// that is not there apart from everything else.
#[cfg(unix)]
#[tokio::test]
async fn local_env_sorts_each_failed_file_call() {
    use EnvErrorKind::{NotFound, Unavailable};

    let (directory, env) = local();
    std::fs::write(directory.path().join("file"), b"x").expect("written");
    std::fs::create_dir(directory.path().join("dir")).expect("made");
    std::os::unix::fs::symlink("file", directory.path().join("link")).expect("linked");

    let kinds = [
        env.read_file("missing", 10)
            .await
            .err()
            .map(|error| error.kind()),
        env.read_file("dir", 10)
            .await
            .err()
            .map(|error| error.kind()),
        env.write_file("file/child", Bytes::from_static(b"x"))
            .await
            .err()
            .map(|error| error.kind()),
        env.write_file("dir", Bytes::from_static(b"x"))
            .await
            .err()
            .map(|error| error.kind()),
        env.append_file("file/child", Bytes::from_static(b"x"))
            .await
            .err()
            .map(|error| error.kind()),
        env.append_file("dir", Bytes::from_static(b"x"))
            .await
            .err()
            .map(|error| error.kind()),
        env.delete_file("missing")
            .await
            .err()
            .map(|error| error.kind()),
        env.list_dir("missing")
            .await
            .err()
            .map(|error| error.kind()),
        env.list_dir("file").await.err().map(|error| error.kind()),
    ];
    let not_regular = [
        env.delete_file("dir")
            .await
            .err()
            .map(|error| error.message().to_owned()),
        env.delete_file("link")
            .await
            .err()
            .map(|error| error.message().to_owned()),
    ];

    assert_eq!(
        kinds,
        [
            Some(NotFound),
            Some(Unavailable),
            Some(Unavailable),
            Some(Unavailable),
            Some(Unavailable),
            Some(Unavailable),
            Some(NotFound),
            Some(NotFound),
            Some(Unavailable),
        ]
    );
    assert_eq!(
        not_regular,
        [
            Some("dir is not a regular file".to_owned()),
            Some("link is not a regular file".to_owned())
        ]
    );
    assert!(
        directory.path().join("file").exists(),
        "the link's target stayed"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn local_env_lists_at_most_a_thousand_entries() {
    let (directory, env) = local();
    for number in 0..1001 {
        std::fs::write(directory.path().join(format!("f{number}")), b"").expect("written");
    }

    let listing = env.list_dir(".").await.expect("listed");

    assert_eq!(listing.entries.len(), 1000);
    assert!(listing.truncated);
}

/// A write or a kill naming a process the environment does not hold is
/// refused; a kill of a running one ends it by signal, and a process the
/// environment started takes what is written to it.
#[cfg(unix)]
#[tokio::test]
async fn local_env_writes_to_and_kills_only_the_processes_it_holds() {
    let (_directory, env) = local();
    let unknown = ProcessId::new(99);

    let write = env.write(unknown, Bytes::from_static(b"x")).await.err();
    let kill = env.kill(unknown).await.err();
    let echo = env
        .spawn(&Spawn::program("head").arg("-c").arg("2"))
        .await
        .expect("started");
    env.write(echo.id, Bytes::from_static(b"hi"))
        .await
        .expect("queued");
    let mut said = Vec::new();
    let echoed = echo
        .ended(|_stream, data| said.extend_from_slice(&data))
        .await;
    let sleeper = env
        .spawn(&Spawn::program("sleep").arg("30"))
        .await
        .expect("started");
    env.kill(sleeper.id).await.expect("held");
    let killed = sleeper.ended(|_, _| {}).await;

    let unknown_process = Some((EnvErrorKind::UnknownProcess, "no such process".to_owned()));
    let read =
        |error: Option<EnvError>| error.map(|error| (error.kind(), error.message().to_owned()));
    assert_eq!(read(write), unknown_process);
    assert_eq!(read(kill), unknown_process);
    assert_eq!(
        (echoed, said.as_slice()),
        (Some(Ending::Exited(0)), b"hi".as_slice())
    );
    assert_eq!(killed, Some(Ending::Signaled(9)));
}

/// A kill that comes after the process's leader exited, while a background
/// process it started still holds the output, kills that process too.
#[cfg(unix)]
#[tokio::test]
async fn local_env_kills_a_background_child_left_after_its_leader_exited() {
    let (directory, env) = local();
    let process = env
        .spawn(&Spawn::program("/bin/sh").arg("-c").arg(LEAVES_A_CHILD))
        .await
        .expect("started");
    let child = written_child(&directory.path().join("child")).await;

    env.kill(process.id)
        .await
        .expect("still held while its output drains");
    let ended = process.ended(|_, _| {}).await;

    assert!(gone(child).await, "the background process was killed");
    assert_eq!(ended, Some(Ending::Signaled(9)));
}

/// A write or an append whose directory cannot be made, because a file sits
/// where it would go, fails in the system's words and makes nothing.
#[cfg(unix)]
#[tokio::test]
async fn local_env_refuses_a_file_under_a_path_a_file_already_holds() {
    let (directory, env) = local();
    std::fs::write(directory.path().join("notes"), "a file").expect("written");

    let written = env
        .write_file("notes/today.txt", Bytes::from_static(b"x"))
        .await
        .err();
    let appended = env
        .append_file("notes/today.txt", Bytes::from_static(b"x"))
        .await
        .err();

    assert_eq!(
        written.map(|error| error.kind()),
        Some(EnvErrorKind::Unavailable)
    );
    assert_eq!(
        appended.map(|error| error.kind()),
        Some(EnvErrorKind::Unavailable)
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("notes"))
            .ok()
            .as_deref(),
        Some("a file")
    );
}

/// A delete the directory's permissions refuse fails in the system's words
/// and leaves the file.
#[cfg(unix)]
#[tokio::test]
async fn local_env_refuses_a_delete_its_directory_forbids() {
    use std::os::unix::fs::PermissionsExt as _;

    let (directory, env) = local();
    let locked = directory.path().join("locked");
    std::fs::create_dir(&locked).expect("made");
    std::fs::write(locked.join("keep.txt"), "kept").expect("written");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("locked");

    let refused = env.delete_file("locked/keep.txt").await.err();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("unlocked");

    assert_eq!(
        refused.map(|error| error.kind()),
        Some(EnvErrorKind::Unavailable)
    );
    assert!(locked.join("keep.txt").is_file(), "the file is still there");
}
