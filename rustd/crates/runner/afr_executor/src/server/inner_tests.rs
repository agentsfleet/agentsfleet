//! An executor with an inner environment: every process it starts enters it
//! in its own working directory, and a renewal and a hosts file reach it; one
//! without refuses both.

#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use rustix::io::Errno;

use super::{Entry, InnerEnvironment, Server, connected};
use crate::api::{Ending, Executor as _, Spawn};
use crate::{Client, Sizes};

/// An inner environment that counts its renewals, remembers where each
/// process was to work, lets each in or refuses it, and holds the hosts file
/// it was given last in place of its own.
#[derive(Debug, Default)]
struct Fake {
    renewed: AtomicUsize,
    entered: Mutex<Vec<PathBuf>>,
    refuses: bool,
    hosts: Mutex<String>,
}

impl InnerEnvironment for Fake {
    fn entry(&self, cwd: &Path) -> io::Result<Entry> {
        self.entered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(cwd.to_owned());
        let refuses = self.refuses;
        // A raw error, built without allocating, as an entry's must be.
        Ok(Box::new(move || {
            if refuses {
                Err(Errno::PERM.into())
            } else {
                Ok(())
            }
        }))
    }

    fn renew(&self) -> io::Result<()> {
        self.renewed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn hosts(&self, file: &str) -> io::Result<()> {
        file.clone_into(&mut self.hosts.lock().unwrap_or_else(PoisonError::into_inner));
        Ok(())
    }
}

/// A client of `server`, which serves `root` on the far end of a socket
/// pair.
fn serving(server: impl FnOnce(std::os::unix::net::UnixStream) -> Server, root: &Path) -> Client {
    let (ours, theirs) = std::os::unix::net::UnixStream::pair().unwrap();
    let root = root.to_owned();
    let server = server(theirs);
    tokio::spawn(async move { server.serve(&root).await });
    Client::connected(ours, Sizes::DEFAULT).unwrap()
}

#[tokio::test]
async fn a_process_enters_the_inner_environment_in_its_working_directory() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let inner = Arc::new(Fake::default());
    let held = Arc::clone(&inner) as Arc<dyn InnerEnvironment>;
    let client = serving(
        |stream| connected(stream).unwrap().with_inner(held),
        dir.path(),
    );

    let process = client
        .spawn(&Spawn::program("true").cwd("sub"))
        .await
        .unwrap();
    let ending = process.ended(|_stream, _data| {}).await;

    assert_eq!(ending, Some(Ending::Exited(0)));
    let entered = inner.entered.lock().unwrap().clone();
    let sub = dir.path().join("sub").canonicalize().unwrap();
    assert_eq!(entered.len(), 1, "{entered:?}");
    assert_eq!(entered.first().unwrap().canonicalize().unwrap(), sub);
}

#[tokio::test]
async fn a_process_that_cannot_enter_the_inner_environment_never_runs() {
    let dir = tempfile::tempdir().unwrap();
    let inner = Arc::new(Fake {
        refuses: true,
        ..Fake::default()
    });
    let client = serving(
        |stream| connected(stream).unwrap().with_inner(inner),
        dir.path(),
    );

    let refused = client
        .spawn(&Spawn::program("touch").arg("ran"))
        .await
        .unwrap_err();

    let permission = io::Error::from(Errno::PERM).to_string();
    assert!(
        refused.wire_message().contains(&permission),
        "the entry's refusal fails the spawn: {}",
        refused.wire_message()
    );
    assert!(!dir.path().join("ran").exists(), "nothing ran outside it");
}

#[tokio::test]
async fn a_renewal_reaches_the_inner_environment_and_answers_once_it_stands() {
    let dir = tempfile::tempdir().unwrap();
    let inner = Arc::new(Fake::default());
    let held = Arc::clone(&inner) as Arc<dyn InnerEnvironment>;
    let client = serving(
        |stream| connected(stream).unwrap().with_inner(held),
        dir.path(),
    );

    client.renew().await.unwrap();
    client.renew().await.unwrap();

    assert_eq!(inner.renewed.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn an_executor_with_no_inner_environment_refuses_a_renewal() {
    let dir = tempfile::tempdir().unwrap();
    let client = serving(|stream| connected(stream).unwrap(), dir.path());

    let refused = client.renew().await.unwrap_err();

    assert!(
        refused
            .wire_message()
            .contains("builds no inner environment"),
        "{}",
        refused.wire_message()
    );
}

#[tokio::test]
async fn a_hosts_file_replaces_the_inner_environments_whole() {
    let dir = tempfile::tempdir().unwrap();
    let inner = Arc::new(Fake::default());
    let held = Arc::clone(&inner) as Arc<dyn InnerEnvironment>;
    let client = serving(
        |stream| connected(stream).unwrap().with_inner(held),
        dir.path(),
    );
    let old = "127.0.0.1 localhost\n203.0.113.7 old.example\n";
    let new = "127.0.0.1 localhost\n198.51.100.9 new.example\n";

    client.hosts(old).await.unwrap();
    client.hosts(new).await.unwrap();

    assert_eq!(*inner.hosts.lock().unwrap(), new, "the old names are gone");
}

#[tokio::test]
async fn an_executor_with_no_inner_environment_refuses_a_hosts_file() {
    let dir = tempfile::tempdir().unwrap();
    let client = serving(|stream| connected(stream).unwrap(), dir.path());

    let refused = client.hosts("127.0.0.1 localhost\n").await.unwrap_err();

    assert!(
        refused
            .wire_message()
            .contains("builds no inner environment"),
        "{}",
        refused.wire_message()
    );
}
