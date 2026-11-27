#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::io;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use afr_engine::Allowlist;
use afr_executor::{Client, Entry, InnerEnvironment, Sizes};

use super::resolve;

/// A guest's inner environment holding the hosts file it was given last.
#[derive(Debug, Default)]
struct Guest {
    hosts: Mutex<String>,
}

impl InnerEnvironment for Guest {
    fn entry(&self, _cwd: &Path) -> io::Result<Entry> {
        Ok(Box::new(|| Ok(())))
    }

    fn renew(&self) -> io::Result<()> {
        Ok(())
    }

    fn hosts(&self, file: &str) -> io::Result<()> {
        file.clone_into(&mut self.hosts.lock().unwrap_or_else(PoisonError::into_inner));
        Ok(())
    }
}

/// An allowed plan changed to another hands the guest the new allowlist's
/// hosts file whole: the new name resolves there, and the old one no longer.
#[tokio::test]
async fn an_allowed_plan_changed_to_another_sends_the_new_hosts_file() {
    let root = tempfile::tempdir().unwrap();
    let guest = Arc::new(Guest::default());
    let (ours, theirs) = std::os::unix::net::UnixStream::pair().unwrap();
    let server = afr_executor::connected(theirs)
        .unwrap()
        .with_inner(Arc::clone(&guest) as Arc<dyn InnerEnvironment>);
    let workspace = root.path().to_owned();
    tokio::spawn(async move { server.serve(&workspace).await });
    let client = Client::connected(ours, Sizes::DEFAULT).unwrap();
    let at = |name: &str, last| (name.to_owned(), Ipv4Addr::new(203, 0, 113, last));
    let first = Allowlist::new(vec![at("first.example", 7)]).unwrap();
    let moved = Allowlist::new(vec![at("moved.example", 9)]).unwrap();

    resolve(&client, &first).await.unwrap();
    resolve(&client, &moved).await.unwrap();

    let held = guest.hosts.lock().unwrap().clone();
    assert_eq!(held, moved.hosts_file());
    assert!(!held.contains("first.example"), "{held}");
}
