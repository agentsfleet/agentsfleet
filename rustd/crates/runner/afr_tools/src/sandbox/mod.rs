//! What the sandbox-side tools share on this runner: the repositories a
//! run's workspace holds, the identity a commit made in the sandbox names,
//! and a scripted executor for the suites.
//!
//! The tools themselves are `guffbolt`'s (`guffbolt::tools::process` and
//! `guffbolt::tools::files`), run through the run's executor as a
//! [`SandboxEnv`](crate::SandboxEnv): none starts a process on the host.
//! Their suites stay here, beside the executor they cross.

mod repositories;
#[cfg(any(test, feature = "test-util"))]
mod scripted;

#[cfg(test)]
mod apply_patch;
#[cfg(test)]
mod browser;
#[cfg(test)]
mod exec_session;
#[cfg(test)]
mod files;
#[cfg(test)]
mod git;
#[cfg(test)]
mod hashed;
#[cfg(test)]
mod image;
#[cfg(test)]
mod oneshot;
#[cfg(test)]
mod read;
#[cfg(test)]
mod sessions;
#[cfg(test)]
mod shell;

pub use self::repositories::{CREDENTIAL_GITHUB, Checkout, checkouts};
#[cfg(any(test, feature = "test-util"))]
pub use self::scripted::{ScriptedExecutor, ScriptedProcess};

/// Who a commit made in the sandbox names as its author and committer, and
/// who the supervisor's checkout names in its reference logs.
pub const GIT_IDENTITY_NAME: &str = "agentsfleet";
/// The address that commit carries: one nobody answers.
pub const GIT_IDENTITY_EMAIL: &str = "noreply@agentsfleet.net";
