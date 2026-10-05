//! Where the runner's telemetry goes, read into a type that cannot carry a
//! credential.
//!
//! The runner reads the same three knobs as the daemon — endpoint, protocol,
//! timeout — and refuses the fourth. A header is how an OTLP exporter carries
//! a credential, and a user in the endpoint is the other way to smuggle one,
//! so both refuse `run` naming the knob: the credential belongs to the runner
//! collector on the host, which a lease cannot read.

use afd_core::env::EnvSource;
use afd_otlp::{OTEL_ENDPOINT_KNOB, OTEL_HEADERS_KNOB, OtlpConfig, Refused};

use crate::error::Result;

#[cfg(test)]
mod tests;

/// Why the runner refuses a credential, as the operator reads it.
pub const NO_CREDENTIAL: &str = "the runner carries no credential; give it to the runner collector";

/// A collector endpoint the runner may export to: no header, no user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint(OtlpConfig);

impl Endpoint {
    /// Resolves the endpoint, or nothing when it is unset.
    ///
    /// The headers knob is refused even when the endpoint is unset: a runner
    /// someone tried to hand a credential to is misconfigured whether or not
    /// it would have exported.
    ///
    /// # Errors
    /// A header knob or a user in the endpoint, naming the knob; any knob
    /// `afd_otlp` cannot read, naming that knob.
    pub fn from_env<E: EnvSource + ?Sized>(env: &E) -> Result<Option<Self>> {
        if afd_otlp::optional(env, OTEL_HEADERS_KNOB).is_some() {
            return Err(refused(OTEL_HEADERS_KNOB).into());
        }
        let Some(config) = OtlpConfig::from_env(env)? else {
            return Ok(None);
        };
        if config.endpoint_names_a_user() {
            return Err(refused(OTEL_ENDPOINT_KNOB).into());
        }
        Ok(Some(Self(config)))
    }

    /// The configuration the pipeline is built from.
    #[must_use]
    pub const fn config(&self) -> &OtlpConfig {
        &self.0
    }
}

/// A credential refused at `knob`.
const fn refused(knob: &'static str) -> Refused {
    Refused {
        knob,
        why: NO_CREDENTIAL,
    }
}
