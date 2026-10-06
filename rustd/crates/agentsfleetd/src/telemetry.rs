//! The daemon's half of the transport: its identity, its log pipeline, and the
//! line that says what is exporting.
//!
//! The pipelines themselves are `afd_otlp`'s, which the runner builds through
//! too; what is left here is what only this binary decides. It bridges its log
//! records as well as its spans, so it asks for the log pipeline the runner
//! does not build, and it names itself `agentsfleetd` on every signal.
//!
//! # What boot does NOT do here
//!
//! Read the environment. Knobs are `preflight`'s and arrive resolved, so this
//! module cannot disagree with the refusal that already happened.

use afd_observability::metrics::instrument::Instruments;
use afd_observability::metrics::registry::Registry;
use afd_observability::semconv;
use afd_otlp::{Builder, OtlpConfig, Service};

use crate::error::BootFailure;

mod flush;
mod resident;

pub use self::flush::flush_within;
pub use afd_otlp::{COLLECT_INTERVAL, Exports};

#[cfg(test)]
mod tests;

pub(crate) use self::resident::resident_bytes;

/// This daemon, as every signal describes it.
const SERVICE: Service = Service::new(semconv::SCOPE_NAME, env!("CARGO_PKG_VERSION"));

/// Builds every pipeline, installs the process-wide handles, and claims the
/// instrument set.
///
/// It does NOT install the gauge producers, which is why it takes no
/// `GaugeSources`: those read boot state that does not exist yet, while the
/// span and log bridges need only the endpoint. That separation is what lets
/// boot attach the exporter BEFORE it opens the pools — see
/// `serve::exporting`, which finishes the job with `producers::install` and
/// [`announce`].
///
/// # Errors
///
/// A census the instrument layer refuses, or an exporter that will not build
/// from the accepted knobs. Both refuse boot: each is a defect that would
/// otherwise present as a collector receiving nothing.
pub fn install(config: &OtlpConfig) -> Result<(Exports, Instruments), BootFailure> {
    let registry = Registry::declared()?;
    // The globals too: the delivery span is opened through
    // `opentelemetry::global::tracer`.
    Ok(Builder::new(config, SERVICE, registry)
        .with_logs()
        .with_global_providers()
        .install()?)
}

/// Says what is exporting, where from, and what nothing feeds.
///
/// The endpoint is named by its SOURCE and never by its value: it is read from
/// the same place as the credential beside it, and a line carrying one is a
/// line a reader will assume carries neither.
pub(crate) fn announce(config: &OtlpConfig, installed: bool, instruments: &Instruments) {
    let source = config.source();
    let protocol = config.encoding().as_str();
    let families = instruments.registry().len() - instruments.unclaimed().len();
    tracing::info!(
        source,
        protocol,
        families,
        installed,
        event = "startup_otel_enabled",
        "telemetry is exporting"
    );
    for row in afd_observability::metrics::produced::UNPRODUCED {
        // `debug`, and once per boot: this is a standing property of the build
        // rather than an event, and an operator reads it when a family they
        // expected is missing.
        tracing::debug!(
            family = row.family,
            reason = row.why,
            event = "metric_family_unproduced",
            "a declared family has no producer in this build"
        );
    }
}
