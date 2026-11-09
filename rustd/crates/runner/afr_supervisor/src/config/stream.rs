//! The native origin and trust policy are independent of the remaining HTTP verbs.

use std::path::PathBuf;

use af_core::env::{EnvSource, optional};
use af_stream::{Endpoint, tls::Trust};

use crate::{Result, error};

/// HTTPS origin of the direct daemon listener; the stream path is fixed.
pub const ENV_STREAM_URL: &str = "AGENTSFLEET_RUNNER_STREAM_URL";
/// Optional public CA certificate bundle added to the operating system roots.
pub const ENV_STREAM_CA: &str = "AGENTSFLEET_RUNNER_STREAM_TLS_CA_CERT_FILE";

/// Validated native daemon origin, explicit routing and trusted issuer roots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamConfig {
    pub(crate) endpoint: Endpoint,
    pub(crate) trust: Trust,
}

impl StreamConfig {
    pub(super) fn from_env(env: &impl EnvSource) -> Result<Self> {
        let origin = optional(env, ENV_STREAM_URL)
            .ok_or_else(|| error::config("AGENTSFLEET_RUNNER_STREAM_URL is not set"))?;
        let endpoint = Endpoint::parse(&origin)?.with_environment_route(env)?;
        let trust = optional(env, ENV_STREAM_CA).map_or_else(Trust::default, |path| {
            Trust::with_roots(PathBuf::from(path.as_ref()))
        });
        Ok(Self { endpoint, trust })
    }
}
