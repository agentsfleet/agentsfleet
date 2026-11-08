//! Many leases on one host: each mirror is fetched by one worker at a time,
//! and a held mirror holds back no other.

#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::path::{Path, PathBuf};

use afr_sandbox::HostWorkspace;
use afr_tools::sandbox::Checkout;
use tokio_util::sync::CancellationToken;

use super::tests::{BASE, Fixture, NAME, OWNER, REPOSITORY, TOKEN};
use super::{Fetched, Request};
use crate::error::Result;
use crate::test_support::head;

/// A second workspace, whose mirror of the same repository is its own.
const OTHER_SCOPE: &str = "ws_2";
/// How many leases race for one mirror.
const LEASES: usize = 100;

/// [`REPOSITORY`] checked out into `workspace` from the mirror `scope` keeps.
async fn check_out_in(fixture: &Fixture, scope: &str, workspace: &Path) -> Result<Fetched> {
    let request = Request {
        scope,
        checkout: Checkout {
            repository: REPOSITORY,
            owner: OWNER,
            name: NAME,
            base: BASE,
        },
        token: TOKEN,
        workspace: HostWorkspace {
            root: workspace,
            owner: fixture.owner,
        },
    };
    fixture
        .mirrors
        .check_out(request, &CancellationToken::new())
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_mirror_holds_back_only_its_own_checkouts() {
    let fixture = Fixture::new();
    let lock = fixture.mirrors.lock(&fixture.mirror());
    let held = lock.lock().await;
    let waiting_in = fixture.workspace("lease_1");
    let other_in = fixture.workspace("lease_2");
    let waiting = fixture.check_out(&waiting_in, BASE);
    tokio::pin!(waiting);

    // Biased, so the held mirror's checkout is polled first every time: it
    // can only lose the race by never finishing while the lock is held.
    let other = tokio::select! {
        biased;
        _ran = &mut waiting => panic!("a checkout ran past its held mirror"),
        other = check_out_in(&fixture, OTHER_SCOPE, &other_in) => other,
    };
    drop(held);
    let waited = waiting.await;

    assert_eq!(other.unwrap(), Fetched::Cloned, "no lock is global");
    assert_eq!(
        waited.unwrap(),
        Fetched::Cloned,
        "the held one runs once freed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_leases_on_one_mirror_clone_it_exactly_once() {
    let fixture = Fixture::new();
    let tip = head(&fixture.remote(), BASE);
    let workspaces: Vec<PathBuf> = (0..LEASES)
        .map(|lease| fixture.workspace(&format!("lease_{lease}")))
        .collect();

    let fetched = futures_util::future::join_all(
        workspaces
            .iter()
            .map(|workspace| fixture.check_out(workspace, BASE)),
    )
    .await;

    let fetched: Vec<Fetched> = fetched.into_iter().map(Result::unwrap).collect();
    let count = |kind| fetched.iter().filter(|each| **each == kind).count();
    assert_eq!(count(Fetched::Cloned), 1, "one clone, however many race");
    assert_eq!(count(Fetched::Unchanged), LEASES - 1);
    for workspace in &workspaces {
        assert_eq!(head(&workspace.join(NAME), "HEAD"), tip);
    }
}
