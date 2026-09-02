//! The subscriber every `tracing` emit in this process needs, and the slot the
//! exporter fills later.
//!
//! `tracing` is the emitting half and this daemon already uses it in 97 places.
//! `tracing_subscriber` is the receiving half: it filters by level, formats,
//! and writes. With none installed the macros are no-ops that do not even
//! evaluate their arguments, which is what a full boot producing one line of
//! output was.
//!
//! # Stderr is unconditional, and that is the whole failure posture
//!
//! Records go to stderr whether or not anything exports. It is the path that
//! works before the exporter is built, after it fails, and in every deployment
//! that configures no collector at all — so an operator debugging a dark
//! backend is reading the same stream they were reading before it went dark.
//!
//! # Why a slot rather than one subscriber built at boot
//!
//! The process-wide default can be set once. It is set in `main`, before a
//! knob is read, so a preflight refusal has somewhere to go; the exporter
//! those knobs describe is built several hundred milliseconds later. A
//! reloadable layer is how the second half arrives without a second
//! `set_global_default` — and it is what lets a deployment with no collector
//! leave the slot empty forever, paying one `Option` per event.

use std::sync::OnceLock;

use afd_core::env::EnvSource;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::{Layer, Registry, reload};

use crate::telemetry::Exports;
use crate::tty::Rendering;

/// The environment variable naming how much to log.
///
/// Its VALUE is a level — `error`, `warn`, `info`, `debug`, `trace`, `off` —
/// so `AGENTSFLEET_LOG_LEVEL=debug agentsfleetd serve`. Not a file: records go
/// to stderr, and where they go from there is the collector's business.
///
/// Spelled in full rather than as a bare `AGENTSFLEET_LOG`, so the name says
/// which knob it is at the call site and in a deployment manifest.
pub const LOG_LEVEL_VAR: &str = "AGENTSFLEET_LOG_LEVEL";

/// Where a record goes when nobody chose.
///
/// `info`, because the lines an incident needs — which routes mounted, which
/// gate refused, which lease issued — are all `info`.
pub const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

/// What the slot holds once an exporter exists.
type Attached = Option<Box<dyn Layer<Registry> + Send + Sync>>;

/// The handle boot fills once it knows where telemetry goes.
///
/// Held by whoever installed the subscriber. A deployment that exports nothing
/// simply never calls [`Signals::attach`], and the slot stays empty for the
/// life of the process.
pub struct Signals(reload::Handle<Attached, Registry>);

impl core::fmt::Debug for Signals {
    /// A boxed layer carries nothing printable, and whether the slot is filled
    /// is the only fact about this a reader could act on — which the boot log
    /// already says.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Signals").finish_non_exhaustive()
    }
}

impl Signals {
    /// Points this process's spans and log records at `exports`.
    ///
    /// Two bridges, because a span and an event are different signals and only
    /// one of them is a trace. Both read the SAME emits this daemon already
    /// writes — nothing is re-instrumented to export, which is what makes the
    /// stderr stream and the exported stream two views of one thing rather
    /// than two things that can disagree.
    ///
    /// Answers whether the slot took it. `false` means the subscriber it
    /// belongs to is gone, which cannot happen at boot and is not worth
    /// failing over if it somehow did.
    #[must_use]
    pub fn attach(&self, exports: &Exports) -> bool {
        let spans = tracing_opentelemetry::layer().with_tracer(
            opentelemetry::trace::TracerProvider::tracer(
                exports.tracer(),
                afd_observability::semconv::SCOPE_NAME,
            ),
        );
        let records = opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(
            exports.logger(),
        );
        self.0
            .modify(|slot| *slot = Some(Box::new(spans.and_then(records))))
            .is_ok()
    }
}

/// The slot this process's subscriber holds, once one is installed.
///
/// A process global because the subscriber is one: `set_global_default` takes
/// the slot once, and threading the handle from `main` through the command
/// line into boot would be four signatures carrying a value that is already
/// unique by construction.
static SIGNALS: OnceLock<Signals> = OnceLock::new();

/// The slot an exporter attaches to, if a subscriber was installed.
#[must_use]
pub fn signals() -> Option<&'static Signals> {
    SIGNALS.get()
}

/// Installs the subscriber for the rest of the process, on stderr.
///
/// Stderr because stdout is already spoken for: the banner and every
/// subcommand's answer are a program interface, and interleaving records into
/// them would corrupt both. An unreadable level falls back rather than
/// refusing — a typo in a debugging aid must not stop a daemon booting.
///
/// Answers whether it took. `false` means something installed one first:
/// ordinary in a test binary, a bug at boot, and in neither case a reason to
/// refuse to serve. The slot it leaves behind is [`signals`].
pub fn install(env: &dyn EnvSource) -> bool {
    let level = env
        .get(LOG_LEVEL_VAR)
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(DEFAULT_LEVEL);
    let (slot, handle) = reload::Layer::new(Attached::None);
    // The slot is layered FIRST, so what it holds is a layer over the bare
    // registry: a reloadable layer's type names the subscriber beneath it, and
    // adding it last would name the whole stack instead.
    let subscriber = Registry::default().with(slot).with(level).with(
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(Rendering::of_stderr() == Rendering::Rich),
    );
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        return false;
    }
    // `is_ok` rather than a bare set: a second call cannot happen after the
    // global default took, and answering the first caller's success is what
    // the boot path reads.
    SIGNALS.set(Signals(handle)).is_ok()
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use afd_observability::producers::GaugeSources;
    use tracing_subscriber::{Registry, reload};

    use super::{Attached, Signals};
    use crate::preflight::{OTEL_ENDPOINT_KNOB, OtlpConfig};
    use crate::telemetry::Exports;

    /// A collector nothing is listening on, as `telemetry/tests.rs` spells it.
    ///
    /// The bridges are BUILT here and never dialled, so an endpoint that
    /// resolved would make this depend on the network for nothing.
    const UNREACHABLE: &str = "http://127.0.0.1:1";

    fn exports() -> Result<Exports, &'static str> {
        crate::telemetry::install(
            &OtlpConfig {
                endpoint: UNREACHABLE.into(),
                source: OTEL_ENDPOINT_KNOB,
                headers: Vec::new(),
                protocol: "http/json".into(),
                timeout: Duration::from_millis(50),
            },
            &GaugeSources::silent(),
        )
        .map_err(|_refused| "a well-formed endpoint builds a transport")
    }

    /// The reload slot takes the bridges, and `Debug` does not unfold it.
    ///
    /// Built from a handle of its own rather than through `install`. The
    /// process-wide slot is won by whoever installs a subscriber FIRST, so a
    /// test that reached `attach` through `logs::signals()` would reach it only
    /// when it happened to win that race — and would pass, silently exercising
    /// nothing, whenever it lost. `integration_telemetry.rs` is written that
    /// way and its early return is why this seam reads as uncovered.
    ///
    /// The `Debug` half is not decoration: a `{:?}` somebody adds later must
    /// not print a subscriber's innards into a log line.
    #[tokio::test]
    async fn the_reload_slot_takes_the_export_bridges() -> Result<(), &'static str> {
        let (slot, handle) = reload::Layer::<Attached, Registry>::new(Attached::None);
        let signals = Signals(handle);

        let rendered = format!("{signals:?}");
        assert!(
            rendered.contains("Signals") && !rendered.contains("Handle"),
            "the rendering names the type without unfolding its handle: {rendered}"
        );

        assert!(
            signals.attach(&exports()?),
            "the live slot accepts the span and record bridges"
        );

        drop(slot);
        Ok(())
    }
}
