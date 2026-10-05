//! Where the runner's telemetry goes, read into a type that cannot carry a
//! credential.
//!
//! The runner reads the same three knobs as the daemon — endpoint, protocol,
//! timeout — and refuses every header knob. A header is how an OTLP exporter
//! carries a credential, and a user in the endpoint is the other way to
//! smuggle one, so both refuse `run` naming the knob: the credential belongs
//! to the runner collector on the host, which a lease cannot read.
//!
//! "Every header knob" is four, not one. The exporter reads each signal's own
//! header knob from the process environment itself, prefers it to the general
//! one, and merges it into every request whatever this crate configured — so
//! checking the general knob alone would leave three ways in.

use afd_core::env::EnvSource;
use afd_otlp::{COMPRESSION_KNOBS, HEADER_KNOBS, OTEL_ENDPOINT_KNOB, OtlpConfig, Refused};

use crate::error::Result;

#[cfg(test)]
mod tests;

/// Why the runner refuses a credential, as the operator reads it.
pub const NO_CREDENTIAL: &str = "the runner carries no credential; give it to the runner collector";

/// Why the runner refuses a compression knob, as the operator reads it.
pub const NO_COMPRESSION: &str =
    "the runner sends uncompressed; leave every compression knob unset";

/// A collector endpoint the runner may export to: no header, no user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint(OtlpConfig);

impl Endpoint {
    /// Resolves the endpoint, or nothing when it is unset.
    ///
    /// A header knob is refused even when the endpoint is unset: a runner
    /// someone tried to hand a credential to is misconfigured whether or not
    /// it would have exported. A compression knob is refused too, because the
    /// exporter would refuse to build on it without naming it.
    ///
    /// # Errors
    /// A header knob or a user in the endpoint, naming the knob; a
    /// compression knob, naming it; any knob `afd_otlp` cannot read, naming
    /// that knob.
    pub fn from_env<E: EnvSource + ?Sized>(env: &E) -> Result<Option<Self>> {
        if let Some(knob) = first_set(env, &HEADER_KNOBS) {
            return Err(refused(knob, NO_CREDENTIAL).into());
        }
        if let Some(knob) = first_set(env, &COMPRESSION_KNOBS) {
            return Err(refused(knob, NO_COMPRESSION).into());
        }
        let Some(config) = OtlpConfig::from_env(env)? else {
            return Ok(None);
        };
        if config.endpoint_names_a_user() {
            return Err(refused(OTEL_ENDPOINT_KNOB, NO_CREDENTIAL).into());
        }
        Ok(Some(Self(config)))
    }

    /// The configuration the pipeline is built from.
    #[must_use]
    pub const fn config(&self) -> &OtlpConfig {
        &self.0
    }
}

/// The first of `knobs` the environment sets to something other than blank.
fn first_set<E: EnvSource + ?Sized>(env: &E, knobs: &[&'static str]) -> Option<&'static str> {
    knobs
        .iter()
        .copied()
        .find(|knob| afd_otlp::optional(env, knob).is_some())
}

/// `knob` refused, for the reason `why`.
const fn refused(knob: &'static str, why: &'static str) -> Refused {
    Refused { knob, why }
}
