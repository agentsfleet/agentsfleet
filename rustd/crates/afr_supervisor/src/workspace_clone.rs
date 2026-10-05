//! The fleet's repository, checked out into the lease's workspace from the
//! host side, so the token that fetched it never enters the sandbox.
//!
//! ```text
//!   fetch into the workspace's mirror  (HTTPS; the token rides an in-memory header)
//!     ─► copy the mirror's objects into <workspace>/<name>/.git
//!     ─► origin = the plain URL, the base branch checked out
//!     ─► every file handed to the sandbox's user
//! ```
//!
//! One mirror per workspace and repository lives under the runner's storage
//! home, so a later lease fetches only what changed. The workspace gets a copy,
//! never a link: nothing outside the sandbox resolves inside it. Two workers
//! never fetch into one mirror at once; each mirror has its own lock.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use afr_sandbox::HostWorkspace;
use afr_tools::sandbox::Checkout;
use base64::Engine as _;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::error::{self, Result};

mod checkout;
mod fetch;

pub use self::fetch::Fetched;

/// Where repositories are fetched from in production.
pub const GITHUB_ORIGIN: &str = "https://github.com/";
/// The suffix a repository's URL and its mirror's directory carry.
const GIT_SUFFIX: &str = ".git";
/// The user GitHub reads an installation token under, over git's HTTPS.
const TOKEN_USER: &str = "x-access-token";
/// The step a failure names when the mirror would not fetch.
const STEP_FETCH: &str = "fetched";
/// The step a failure names when the workspace copy would not check out.
const STEP_CHECKOUT: &str = "checked out";

/// The repository mirrors on this host, and a lock per mirror.
#[derive(Debug)]
pub struct Mirrors {
    root: PathBuf,
    origin: String,
    locks: Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
}

/// One checkout's inputs.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The workspace the lease's fleet belongs to, which keys its mirrors.
    pub scope: &'a str,
    /// Which repository, at which branch.
    pub checkout: Checkout<'a>,
    /// The token the fetch presents; it stays on this host.
    pub token: &'a str,
    /// The lease's workspace, as the host sees it.
    pub workspace: HostWorkspace<'a>,
}

impl Mirrors {
    /// Mirrors kept under `root`, fetched from `origin`, which ends in `/`.
    #[must_use]
    pub fn new(root: PathBuf, origin: impl Into<String>) -> Self {
        Self {
            root,
            origin: origin.into(),
            locks: Mutex::new(HashMap::new()),
        }
    }

    /// Brings `request`'s mirror up to date and checks the repository out
    /// into the lease's workspace; what the fetch found.
    ///
    /// # Errors
    /// The fetch or the checkout failed, or `interrupt` stopped it.
    pub async fn check_out(
        &self,
        request: Request<'_>,
        interrupt: &CancellationToken,
    ) -> Result<Fetched> {
        let Checkout {
            repository,
            owner,
            name,
            base,
        } = request.checkout;
        let mirror = self
            .root
            .join(request.scope)
            .join(owner)
            .join(format!("{name}{GIT_SUFFIX}"));
        let url = format!("{}{repository}{GIT_SUFFIX}", self.origin);
        let header = authorization(request.token);
        let lock = self.lock(&mirror);
        let _held = lock.lock().await;
        let stop = Arc::new(AtomicBool::new(false));
        let fetched = {
            let (url, mirror, stop_fetch) = (url.clone(), mirror.clone(), Arc::clone(&stop));
            blocking(interrupt, &stop, move || {
                fetch::fetch(&url, &mirror, &header, &stop_fetch)
            })
            .await
            .map_err(error::git(repository, STEP_FETCH))?
        };
        let destination = request.workspace.root.join(name);
        let owner_ids = request.workspace.owner;
        let base = base.to_owned();
        let stop_checkout = Arc::clone(&stop);
        blocking(interrupt, &stop, move || {
            checkout::check_out(
                &mirror,
                &destination,
                &url,
                &base,
                owner_ids,
                &stop_checkout,
            )
        })
        .await
        .map_err(error::git(repository, STEP_CHECKOUT))?;
        Ok(fetched)
    }

    /// The lock for `mirror`, made the first time anyone asks for it.
    fn lock(&self, mirror: &Path) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry(mirror.to_owned()).or_default())
    }
}

/// The header that presents `token` the way GitHub reads one over git's
/// HTTPS: basic credentials naming the installation-token user. Wiped when
/// dropped, as the token itself is.
fn authorization(token: &str) -> Zeroizing<String> {
    let credentials = Zeroizing::new(format!("{TOKEN_USER}:{token}"));
    let encoded =
        Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes()));
    Zeroizing::new(format!("Authorization: Basic {}", encoded.as_str()))
}

/// Runs `work` on a blocking thread. `interrupt` raises `stop`, which the git
/// library reads between steps, and the work is still waited for, so nothing
/// writes into a workspace being torn down.
async fn blocking<T: Send + 'static>(
    interrupt: &CancellationToken,
    stop: &AtomicBool,
    work: impl FnOnce() -> fetch::GitResult<T> + Send + 'static,
) -> fetch::GitResult<T> {
    let task = tokio::task::spawn_blocking(work);
    tokio::pin!(task);
    let joined = tokio::select! {
        joined = &mut task => joined,
        () = interrupt.cancelled() => {
            stop.store(true, Ordering::Relaxed);
            (&mut task).await
        }
    };
    joined.map_err(fetch::GitError::from)?
}

#[cfg(test)]
#[path = "workspace_clone/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "workspace_clone/failure_tests.rs"]
mod failure_tests;

#[cfg(test)]
#[path = "workspace_clone/http_tests.rs"]
mod http_tests;

#[cfg(test)]
#[path = "workspace_clone/concurrency_tests.rs"]
mod concurrency_tests;
