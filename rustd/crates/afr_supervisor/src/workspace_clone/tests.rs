#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use afr_sandbox::HostWorkspace;
use afr_tools::sandbox::Checkout;
use base64::Engine as _;
use tokio_util::sync::CancellationToken;

use super::{Fetched, Mirrors, Request, authorization};
use crate::test_support::{FIRST_README, FIXTURE_BRANCH, commit, git, repository};

pub(super) const SCOPE: &str = "ws_1";
pub(super) const REPOSITORY: &str = "acme/widget";
pub(super) const OWNER: &str = "acme";
pub(super) const NAME: &str = "widget";
pub(super) const BASE: &str = FIXTURE_BRANCH;
pub(super) const TOKEN: &str = "ghs_fixtureTokenNeverWritten";
/// Where the fixture serves repositories from, and keeps their mirrors.
pub(super) const ORIGINS: &str = "origin";
const MIRRORS: &str = "mirrors";

/// The directory a bare copy of [`NAME`] goes by.
fn bare() -> String {
    format!("{NAME}.git")
}

/// Every file under `path`, `path` included, never following a link.
pub(super) fn walk(path: &Path) -> Vec<PathBuf> {
    let mut found = vec![path.to_owned()];
    if path.symlink_metadata().unwrap().is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            found.extend(walk(&entry.unwrap().path()));
        }
    }
    found
}

/// A remote repository with one commit on `main`, served over `file://`, and
/// the mirrors that fetch it.
pub(super) struct Fixture {
    pub(super) root: tempfile::TempDir,
    pub(super) mirrors: Mirrors,
    pub(super) owner: (u32, u32),
}

impl Fixture {
    pub(super) fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let origin = root.path().join(ORIGINS);
        let remote = origin.join(OWNER).join(bare());
        repository(&remote);
        let mirrors = Mirrors::new(
            root.path().join(MIRRORS),
            format!("file://{}/", origin.display()),
        );
        let metadata = fs::metadata(root.path()).unwrap();
        Self {
            root,
            mirrors,
            owner: (metadata.uid(), metadata.gid()),
        }
    }

    pub(super) fn remote(&self) -> PathBuf {
        self.root.path().join(ORIGINS).join(OWNER).join(bare())
    }

    pub(super) fn mirror(&self) -> PathBuf {
        self.root
            .path()
            .join(MIRRORS)
            .join(SCOPE)
            .join(OWNER)
            .join(bare())
    }

    /// A fresh workspace for the lease named `lease`.
    pub(super) fn workspace(&self, lease: &str) -> PathBuf {
        let workspace = self.root.path().join("workspaces").join(lease);
        fs::create_dir_all(&workspace).unwrap();
        workspace
    }

    pub(super) async fn check_out(
        &self,
        workspace: &Path,
        base: &str,
    ) -> crate::error::Result<Fetched> {
        let request = Request {
            scope: SCOPE,
            checkout: Checkout {
                repository: REPOSITORY,
                owner: OWNER,
                name: NAME,
                base,
            },
            token: TOKEN,
            workspace: HostWorkspace {
                root: workspace,
                owner: self.owner,
            },
        };
        self.mirrors
            .check_out(request, &CancellationToken::new())
            .await
    }
}

#[tokio::test]
async fn the_first_checkout_clones_and_lands_on_the_base_head() {
    let fixture = Fixture::new();
    let workspace = fixture.workspace("lease_1");

    let fetched = fixture.check_out(&workspace, BASE).await.unwrap();

    let copy = workspace.join(NAME);
    assert_eq!(fetched, Fetched::Cloned);
    assert_eq!(
        git(&copy, &["rev-parse", "HEAD"]),
        git(&fixture.remote(), &["rev-parse", BASE])
    );
    assert_eq!(git(&copy, &["symbolic-ref", "--short", "HEAD"]), BASE);
    assert_eq!(git(&copy, &["status", "--porcelain"]), "", "a clean tree");
    assert_eq!(
        git(&copy, &["rev-parse", "--abbrev-ref", "@{upstream}"]),
        "origin/main"
    );
    assert_eq!(
        git(&copy, &["config", "remote.origin.url"]),
        format!(
            "file://{}/{REPOSITORY}.git",
            fixture.root.path().join(ORIGINS).display()
        )
    );
    assert_eq!(
        fs::read_to_string(copy.join("README.md")).unwrap(),
        FIRST_README
    );
    assert_eq!(
        git(
            &copy,
            &["for-each-ref", "--format=%(refname)", "refs/remotes"]
        ),
        "refs/remotes/origin/main",
        "origin's branches, and no origin/HEAD frozen at clone time"
    );
    assert_eq!(
        git(
            &copy,
            &["log", "--walk-reflogs", "-1", "--format=%gn <%ge>", BASE]
        ),
        "agentsfleet <noreply@agentsfleet.net>"
    );
}

#[tokio::test]
async fn test_supervisor_clones_at_base_with_cache() {
    let fixture = Fixture::new();
    let head = git(&fixture.remote(), &["rev-parse", BASE]);
    let first = fixture.workspace("lease_1");
    let second = fixture.workspace("lease_2");

    let cloned = fixture.check_out(&first, BASE).await.unwrap();
    let cached = fixture.check_out(&second, BASE).await.unwrap();

    assert_eq!(cloned, Fetched::Cloned);
    assert_eq!(
        cached,
        Fetched::Unchanged,
        "the warm mirror fetches nothing"
    );
    for workspace in [first, second] {
        assert_eq!(git(&workspace.join(NAME), &["rev-parse", "HEAD"]), head);
    }
}

#[tokio::test]
async fn a_pushed_commit_updates_the_mirror_and_the_next_checkout() {
    let fixture = Fixture::new();
    fixture
        .check_out(&fixture.workspace("lease_1"), BASE)
        .await
        .unwrap();
    commit(&fixture.remote(), "README.md", "second");
    let second = fixture.workspace("lease_2");

    let fetched = fixture.check_out(&second, BASE).await.unwrap();

    let copy = second.join(NAME);
    assert_eq!(fetched, Fetched::Updated);
    assert_eq!(
        git(&copy, &["rev-parse", "HEAD"]),
        git(&fixture.remote(), &["rev-parse", BASE]),
        "the checkout follows the fetch, not the branch the clone first wrote"
    );
    assert_eq!(
        fs::read_to_string(copy.join("README.md")).unwrap(),
        "second"
    );
}

#[tokio::test]
async fn a_base_other_than_the_default_branch_is_checked_out() {
    let fixture = Fixture::new();
    let remote = fixture.remote();
    git(&remote, &["checkout", "--quiet", "-b", "release"]);
    commit(&remote, "CHANGELOG.md", "release notes");
    git(&remote, &["checkout", "--quiet", BASE]);
    let workspace = fixture.workspace("lease_1");

    fixture.check_out(&workspace, "release").await.unwrap();

    let copy = workspace.join(NAME);
    assert_eq!(
        git(&copy, &["rev-parse", "HEAD"]),
        git(&remote, &["rev-parse", "release"])
    );
    assert_eq!(git(&copy, &["symbolic-ref", "--short", "HEAD"]), "release");
    assert_eq!(
        git(&copy, &["rev-parse", "--abbrev-ref", "@{upstream}"]),
        "origin/release"
    );
    assert_eq!(
        git(&copy, &["rev-parse", "origin/main"]),
        git(&remote, &["rev-parse", BASE]),
        "every fetched branch is recorded as origin's"
    );
}

/// A read binding names no base, so the checkout takes the remote's default
/// branch, as `git clone` does; `trunk` here, so `main` cannot pass for it.
#[tokio::test]
async fn a_binding_with_no_base_checks_out_the_default_branch() {
    let fixture = Fixture::new();
    let remote = fixture.remote();
    git(&remote, &["checkout", "--quiet", "-b", "trunk"]);
    commit(&remote, "TRUNK.md", "trunk notes");
    let workspace = fixture.workspace("lease_1");

    fixture.check_out(&workspace, "").await.unwrap();

    let copy = workspace.join(NAME);
    assert_eq!(git(&copy, &["symbolic-ref", "--short", "HEAD"]), "trunk");
    assert_eq!(
        git(&copy, &["rev-parse", "HEAD"]),
        git(&remote, &["rev-parse", "trunk"])
    );
    assert_eq!(
        git(&copy, &["rev-parse", "--abbrev-ref", "@{upstream}"]),
        "origin/trunk"
    );
    assert_eq!(
        fs::read_to_string(copy.join("TRUNK.md")).unwrap(),
        "trunk notes"
    );
}

/// A tag on the remote, annotated or not, reaches the working copy, so a
/// build that reads its version from `git describe` finds it.
#[tokio::test]
async fn a_tag_on_the_remote_reaches_the_working_copy() {
    let fixture = Fixture::new();
    let remote = fixture.remote();
    git(&remote, &["tag", "-a", "v1.2.3", "-m", "release 1.2.3"]);
    git(&remote, &["tag", "lightweight"]);
    let workspace = fixture.workspace("lease_1");

    fixture.check_out(&workspace, BASE).await.unwrap();

    let copy = workspace.join(NAME);
    let head = git(&remote, &["rev-parse", BASE]);
    assert_eq!(git(&copy, &["rev-parse", "v1.2.3^{}"]), head);
    assert_eq!(git(&copy, &["rev-parse", "lightweight"]), head);
    assert_eq!(
        git(&copy, &["describe", "--tags", "--exact-match"]),
        "v1.2.3"
    );
}

#[tokio::test]
async fn every_file_is_handed_to_the_owner() {
    let fixture = Fixture::new();
    let workspace = fixture.workspace("lease_1");

    fixture.check_out(&workspace, BASE).await.unwrap();

    for file in walk(&workspace.join(NAME)) {
        let metadata = file.symlink_metadata().unwrap();
        assert_eq!(
            (metadata.uid(), metadata.gid()),
            fixture.owner,
            "{}",
            file.display()
        );
    }
}

#[test]
fn the_header_is_basic_credentials_for_the_installation_token_user() {
    let header = authorization("t0k3n");

    let encoded = header.strip_prefix("Authorization: Basic ").unwrap();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    assert_eq!(decoded, b"x-access-token:t0k3n");
}

#[test]
fn fetched_reads_as_its_log_word() {
    assert_eq!(Fetched::Cloned.as_str(), "cloned");
    assert_eq!(Fetched::Updated.as_str(), "updated");
    assert_eq!(Fetched::Unchanged.as_str(), "unchanged");
}
