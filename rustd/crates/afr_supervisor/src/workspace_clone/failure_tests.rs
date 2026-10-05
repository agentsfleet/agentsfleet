#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::tests::{BASE, Fixture, NAME, ORIGINS, OWNER, REPOSITORY, SCOPE, TOKEN, walk};
use super::{Fetched, Mirrors, authorization, blocking, fetch};
use crate::test_support::git;

#[tokio::test]
async fn the_token_is_written_nowhere() {
    let fixture = Fixture::new();
    let workspace = fixture.workspace("lease_1");

    fixture.check_out(&workspace, BASE).await.unwrap();

    let header = authorization(TOKEN);
    for file in walk(&fixture.mirror()).into_iter().chain(walk(&workspace)) {
        if file.symlink_metadata().unwrap().is_file() {
            let bytes = fs::read(&file).unwrap();
            for secret in [TOKEN, header.as_str()] {
                assert!(
                    !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                    "{} holds a credential",
                    file.display()
                );
            }
        }
    }
}

#[tokio::test]
async fn a_missing_base_branch_refuses() {
    let fixture = Fixture::new();

    let refused = fixture
        .check_out(&fixture.workspace("lease_1"), "nope")
        .await
        .unwrap_err();

    assert_eq!(
        refused.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
    let message = refused.to_string();
    assert!(
        message.contains(REPOSITORY) && message.contains("checked out"),
        "{message}"
    );
}

#[tokio::test]
async fn an_unreadable_mirror_is_replaced() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.mirror()).unwrap();
    fs::write(fixture.mirror().join("junk"), "not a repository").unwrap();
    let workspace = fixture.workspace("lease_1");

    let fetched = fixture.check_out(&workspace, BASE).await.unwrap();

    assert_eq!(fetched, Fetched::Cloned);
    assert!(!fixture.mirror().join("junk").exists());
}

#[tokio::test]
async fn an_unreachable_origin_keeps_the_mirror() {
    let fixture = Fixture::new();
    fixture
        .check_out(&fixture.workspace("lease_1"), BASE)
        .await
        .unwrap();
    let head = git(&fixture.remote(), &["rev-parse", BASE]);
    fs::rename(fixture.remote(), fixture.root.path().join("gone")).unwrap();

    let refused = fixture
        .check_out(&fixture.workspace("lease_2"), BASE)
        .await
        .unwrap_err();

    assert!(refused.to_string().contains("fetched"), "{refused}");
    assert_eq!(git(&fixture.mirror(), &["rev-parse", "origin/main"]), head);
}

#[tokio::test]
async fn an_interrupt_raises_stop_and_still_waits_for_the_work() {
    let interrupt = CancellationToken::new();
    interrupt.cancel();
    let stop = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&stop);
    let finished = Arc::new(AtomicBool::new(false));
    let marked = Arc::clone(&finished);

    let work = move || {
        while !seen.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(1));
        }
        marked.store(true, Ordering::Relaxed);
        Err("interrupted".into())
    };
    let outcome = tokio::time::timeout(
        Duration::from_secs(10),
        blocking::<()>(&interrupt, &stop, work),
    )
    .await
    .unwrap();

    assert!(outcome.is_err());
    assert!(stop.load(Ordering::Relaxed), "the interrupt raised stop");
    assert!(finished.load(Ordering::Relaxed), "the work was waited for");
}

#[test]
fn the_lock_is_one_per_mirror() {
    let mirrors = Mirrors::new(PathBuf::from("/mirrors"), "https://example.com/");

    let first = mirrors.lock(Path::new("/mirrors/a"));
    let again = mirrors.lock(Path::new("/mirrors/a"));
    let other = mirrors.lock(Path::new("/mirrors/b"));

    assert!(Arc::ptr_eq(&first, &again));
    assert!(!Arc::ptr_eq(&first, &other));
}

#[tokio::test]
async fn a_failed_first_clone_leaves_nothing_a_retry_trips_on() {
    let fixture = Fixture::new();
    let hidden = fixture.root.path().join("hidden");
    fs::rename(fixture.remote(), &hidden).unwrap();

    let refused = fixture
        .check_out(&fixture.workspace("lease_1"), BASE)
        .await
        .unwrap_err();
    fs::rename(&hidden, fixture.remote()).unwrap();
    let retried = fixture.check_out(&fixture.workspace("lease_2"), BASE).await;

    assert!(refused.to_string().contains("fetched"), "{refused}");
    assert_eq!(retried.unwrap(), Fetched::Cloned, "the retry clones whole");
}

/// The stop flag is raised before the clone starts: the one deterministic way
/// to interrupt the library mid-fetch, which the public surface reaches only
/// through a race with the blocking thread.
#[tokio::test]
async fn an_interrupted_clone_leaves_nothing_a_retry_trips_on() {
    let fixture = Fixture::new();
    let url = format!(
        "file://{}/{REPOSITORY}.git",
        fixture.root.path().join(ORIGINS).display()
    );

    let interrupted = fetch::fetch(
        &url,
        &fixture.mirror(),
        "X-Unused: 1",
        &AtomicBool::new(true),
    );
    let retried = fixture.check_out(&fixture.workspace("lease_1"), BASE).await;

    assert!(
        interrupted.is_err(),
        "a raised stop ends the fetch: {interrupted:?}"
    );
    assert_eq!(retried.unwrap(), Fetched::Cloned);
}

#[tokio::test]
async fn an_origin_that_is_not_a_url_fails_the_fetch_and_makes_no_mirror() {
    let fixture = Fixture::new();
    let mirrors = Mirrors::new(fixture.root.path().join("bad"), "https://[::1/");
    let workspace = fixture.workspace("lease_1");
    let request = super::Request {
        scope: SCOPE,
        checkout: afr_tools::sandbox::Checkout {
            repository: REPOSITORY,
            owner: OWNER,
            name: NAME,
            base: BASE,
        },
        token: TOKEN,
        workspace: afr_sandbox::HostWorkspace {
            root: &workspace,
            owner: fixture.owner,
        },
    };

    let refused = mirrors
        .check_out(request, &tokio_util::sync::CancellationToken::new())
        .await
        .unwrap_err();

    assert!(
        refused.to_string().contains("could not be fetched"),
        "{refused}"
    );
    assert!(
        std::error::Error::source(&refused).is_some(),
        "the library's reason is kept"
    );
    assert!(
        !fixture
            .root
            .path()
            .join("bad")
            .join(SCOPE)
            .join(OWNER)
            .join(format!("{NAME}.git"))
            .exists()
    );
}
