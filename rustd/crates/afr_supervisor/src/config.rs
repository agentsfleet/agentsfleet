//! What the runner is told at boot: where the daemon is, and who it is.
//!
//! Three variables and nothing else (`docs/architecture/runner_fleet.md`
//! §"Registering a runner"): no policy rides the environment, and no datastore
//! secret ever reaches a host.

use std::fmt;
use std::path::{Path, PathBuf};

use afd_core::env::EnvSource;
use afd_wire::paths::RUNNER_TOKEN_PREFIX;

use crate::error::{self, Result};

/// The daemon's base address.
pub const ENV_API_URL: &str = "AGENTSFLEET_API_URL";
/// The runner's `agt_r` token, minted once by a platform admin.
pub const ENV_RUNNER_TOKEN: &str = "AGENTSFLEET_RUNNER_TOKEN";
/// The host-local root for the spool, the bundle cache and lease scratch.
pub const ENV_STORAGE_HOME: &str = "RUNNER_STORAGE_HOME";
/// Where the storage home lives when none is set: a directory that survives a
/// reboot, because the report spool exists to outlive one.
pub const DEFAULT_STORAGE_HOME: &str = "/var/lib/agentsfleet-runner";

const DETAIL_API_URL_MISSING: &str = "AGENTSFLEET_API_URL is not set";
const DETAIL_API_URL_INVALID: &str = "AGENTSFLEET_API_URL is not an http or https address";
const DETAIL_TOKEN_MISSING: &str = "AGENTSFLEET_RUNNER_TOKEN is not set";
const DETAIL_TOKEN_SHAPE: &str = "AGENTSFLEET_RUNNER_TOKEN is not an agt_r runner token";
/// The schemes a daemon address may use.
const SCHEMES: [&str; 2] = ["http://", "https://"];

/// The runner's token. Its `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct RunnerToken(String);

impl RunnerToken {
    /// The token, for the one header that carries it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RunnerToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// What a redacted secret prints as.
const REDACTED: &str = "RunnerToken(redacted)";

/// The runner's configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    api_url: String,
    token: RunnerToken,
    storage_home: PathBuf,
}

impl Config {
    /// Reads the configuration, refusing loudly on anything missing or
    /// malformed, so a misinstalled host fails at boot rather than in a 401
    /// loop.
    ///
    /// # Errors
    /// A missing or malformed address or token.
    pub fn from_env(env: &impl EnvSource) -> Result<Self> {
        let api_url =
            present(env, ENV_API_URL).ok_or_else(|| error::config(DETAIL_API_URL_MISSING))?;
        if !SCHEMES.iter().any(|scheme| api_url.starts_with(scheme)) {
            return Err(error::config(DETAIL_API_URL_INVALID));
        }
        let token =
            present(env, ENV_RUNNER_TOKEN).ok_or_else(|| error::config(DETAIL_TOKEN_MISSING))?;
        if !token.starts_with(RUNNER_TOKEN_PREFIX) {
            return Err(error::config(DETAIL_TOKEN_SHAPE));
        }
        let storage_home = present(env, ENV_STORAGE_HOME)
            .map_or_else(|| PathBuf::from(DEFAULT_STORAGE_HOME), PathBuf::from);
        Ok(Self {
            api_url,
            token: RunnerToken(token),
            storage_home,
        })
    }

    /// The daemon's base address.
    #[must_use]
    pub fn api_url(&self) -> &str {
        &self.api_url
    }

    /// The runner's token.
    #[must_use]
    pub const fn token(&self) -> &RunnerToken {
        &self.token
    }

    /// The host-local storage root.
    #[must_use]
    pub fn storage_home(&self) -> &Path {
        &self.storage_home
    }
}

/// A variable's value, with a blank one read as unset.
fn present(env: &impl EnvSource, key: &str) -> Option<String> {
    env.get(key).filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
#[path = "config/tests.rs"]
mod tests;
