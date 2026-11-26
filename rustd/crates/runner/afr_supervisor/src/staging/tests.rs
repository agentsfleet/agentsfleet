//! Checkout and recovery cross into a workspace the host has no path to,
//! through the executor's `import` and `export` (Dimension 3.3).

#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::time::Duration;

use af_core::id::Uuid7;
use afr_executor::{Client, Sizes};
use afr_tools::sandbox::Checkout;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::Staging;
use crate::test_support::{self, FIXTURE_BRANCH, git, repository};
use crate::workspace_clone::{Mirrors, Request};

const OWNER: &str = "acme";
const NAME: &str = "widget";
const REPOSITORY: &str = "acme/widget";
const SCOPE: &str = "01890a5d-ac96-774b-bcce-b302099a805a";
const TOKEN: &str = "ghs_fixtureTokenNeverWritten";

/// A request for the fixture repository at its base branch.
const fn request() -> Request<'static> {
    Request {
        scope: SCOPE,
        checkout: Checkout {
            repository: REPOSITORY,
            owner: OWNER,
            name: NAME,
            base: FIXTURE_BRANCH,
        },
        token: TOKEN,
    }
}

/// SHA-256 over every entry under `root` but those named `left_out`, in name
/// order: its path, its kind, its permission bits, and its content or link
/// target. A link's own mode means nothing and is left out.
fn digest(root: &Path, left_out: &str) -> String {
    fn walk(root: &Path, at: &Path, left_out: &str, listing: &mut Vec<u8>) {
        let mut names: Vec<_> = fs::read_dir(root.join(at))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.as_os_str() != left_out)
            .collect();
        names.sort();
        for name in names {
            let path = at.join(name);
            let full = root.join(&path);
            let found = full.symlink_metadata().unwrap();
            listing.extend_from_slice(path.as_os_str().as_encoded_bytes());
            if found.is_symlink() {
                listing.extend_from_slice(b"\0link\0");
                let target = fs::read_link(&full).unwrap();
                listing.extend_from_slice(target.as_os_str().as_encoded_bytes());
            } else {
                let mode = found.permissions().mode() & 0o777;
                listing.extend_from_slice(format!("\0{mode:o}\0").as_bytes());
                if found.is_dir() {
                    walk(root, &path, left_out, listing);
                } else {
                    listing.extend_from_slice(&fs::read(&full).unwrap());
                }
            }
            listing.push(b'\n');
        }
    }
    let mut listing = Vec::new();
    walk(root, Path::new(""), left_out, &mut listing);
    af_core::etag::sha256_hex(&listing)
}

/// How long the executor may take to bind its socket.
const BOUND_WITHIN: Duration = Duration::from_secs(1);

/// A sandbox's workspace as the host meets it: a real executor serving a
/// directory over a socket, and nothing handed to the host to reach it by.
struct Served {
    id: Uuid7,
    client: Client,
    server: JoinHandle<afr_executor::Result<()>>,
}

impl Served {
    /// A fresh workspace under `base`, served.
    async fn start(base: &Path) -> Self {
        let id = afr_engine::fresh_sandbox_id();
        let dir = base.join(id.as_str());
        let workspace = dir.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let socket = dir.join("executor.sock");
        let server = tokio::spawn({
            let socket = socket.clone();
            async move { afr_executor::bind(&socket)?.serve(&workspace).await }
        });
        let client = Client::connect_within(&socket, BOUND_WITHIN, Sizes::DEFAULT)
            .await
            .unwrap();
        Self { id, client, server }
    }

    const fn id(&self) -> &Uuid7 {
        &self.id
    }

    const fn executor(&self) -> &Client {
        &self.client
    }

    /// Hangs up, which ends the executor's session.
    async fn stop(self) {
        drop(self.client);
        self.server.await.unwrap().unwrap();
    }
}

// Value: protects=checkout and checkpoint files cross into the sandbox with no
// host path; fails_when=an import or export drops, reorders or remodes a file,
// or a checkout imports a different tree than the host checkout makes;
#[tokio::test]
async fn test_checkout_fills_a_workspace_the_host_cannot_see() {
    // Short: the executor's socket path is capped near a hundred bytes.
    let root = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    let origin = root.path().join("origin");
    let remote = origin.join(OWNER).join(format!("{NAME}.git"));
    repository(&remote);
    fs::create_dir(remote.join("bin")).unwrap();
    fs::write(remote.join("bin/crow"), "#!/bin/sh\necho crow\n").unwrap();
    fs::set_permissions(remote.join("bin/crow"), fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink("../README.md", remote.join("bin/readme")).unwrap();
    git(&remote, &["add", "bin"]);
    git(&remote, &["commit", "--quiet", "--message", "tools"]);
    let mirrors = Mirrors::new(
        root.path().join("mirrors"),
        format!("file://{}/", origin.display()),
    );
    let staging = Staging::sweep(&root.path().join("staging")).unwrap();
    let sandboxes = root.path().join("sandboxes");
    let stop = CancellationToken::new();
    let today = root.path().join("today");
    fs::create_dir(&today).unwrap();
    mirrors.check_out(request(), &today, &stop).await.unwrap();

    let first = Served::start(&sandboxes).await;
    let staged = staging.stage(first.id()).unwrap();
    mirrors
        .land(request(), &staged, first.executor(), &stop)
        .await
        .unwrap();
    drop(staged);
    let captured = staging.stage(first.id()).unwrap();
    captured.export(first.executor()).await.unwrap();
    let second = Served::start(&sandboxes).await;
    captured.restore(second.executor()).await.unwrap();
    let restored = staging.stage(second.id()).unwrap();
    restored.export(second.executor()).await.unwrap();
    let refused = captured.restore(second.executor()).await;
    first.stop().await;
    second.stop().await;

    let copy = captured.path().join(NAME);
    // Read before git runs in the copy: `status` refreshes its index.
    let (captured_tree, restored_tree) = (digest(captured.path(), ""), digest(restored.path(), ""));
    assert_eq!(
        restored_tree, captured_tree,
        "a restore lands what the capture read, byte for byte"
    );
    assert_eq!(
        digest(&copy, ".git"),
        digest(&today.join(NAME), ".git"),
        "the checkout imports the tree a host checkout makes"
    );
    assert_eq!(
        git(&copy, &["rev-parse", "HEAD"]),
        git(&remote, &["rev-parse", FIXTURE_BRANCH]),
        "the workspace holds the base head"
    );
    assert_eq!(git(&copy, &["status", "--porcelain"]), "", "a clean tree");
    assert_eq!(
        test_support::detail(&refused),
        Some("checkpoint restore requires a fresh empty workspace"),
        "a restore lands only in an empty workspace"
    );
    let stage = captured.path().to_owned();
    drop(captured);
    assert!(!stage.exists(), "a stage goes when its step ends");
}
