//! Where telemetry is switched on, and what the two boot-owned gauges read.
//!
//! Split from `serve` because boot's job is to open things and this is one
//! thing it opens: the export pipelines, the sampler that keeps the resident
//! gauge fresh, and the flush that runs when the process is asked to stop.

use std::sync::Arc;

use afd_api::Admission;
use afd_observability::producers::GaugeSources;

use crate::error::BootFailure;
use crate::preflight::BootConfig;
use crate::supervisor::Supervisor;

/// What the two boot-owned gauges read.
///
/// Both are a lock-free load on a value boot already holds, which is what
/// makes them safe inside a collection callback: the SDK runs those under its
/// own pipeline lock, with no timeout, so a reading that could block would
/// take every family silent at once rather than slow one down.
///
/// The resident set is NOT here for exactly that reason — it comes from
/// `/proc`, so the export task samples it and publishes it into a cell the
/// callback loads.
pub(super) fn gauge_sources(admission: &Admission, live: &afd_sse::Live) -> GaugeSources {
    let requests = admission.clone();
    let streams = live.clone();
    GaugeSources {
        requests_in_flight: Arc::new(move || u64::try_from(requests.in_flight()).ok()),
        streams_in_flight: Arc::new(move || u64::try_from(streams.carrying()).ok()),
    }
}

/// Builds the export pipelines and supervises their flush, where a collector
/// is configured.
///
/// A deployment that named none boots, serves, and exports nothing — which is
/// every developer's environment and most tests. The task is spawned only in
/// the configured case, which is why the daemon's inventory carries it
/// conditionally and `integration_serve.rs` says so.
pub(crate) fn open_telemetry(
    config: &BootConfig,
    supervisor: &mut Supervisor,
    sources: &GaugeSources,
) -> Result<(), BootFailure> {
    let Some(otlp) = config.otlp() else {
        // The Zig daemon's own event name and reason field, kept: a dashboard
        // or an alert matching on this line matches it from either binary.
        tracing::info!(
            reason = "no endpoint configured",
            event = "startup_otel_disabled",
            "telemetry is not exporting"
        );
        return Ok(());
    };

    let exports = crate::telemetry::install(otlp, sources)?;
    if let Some(signals) = crate::logs::signals() {
        let attached = signals.attach(&exports);
        tracing::debug!(attached, event = "telemetry_layers_attached");
    }
    supervisor.spawn(crate::OTLP_EXPORT, move |token| async move {
        // The resident-set sampler, on the reader's own cadence so the gauge
        // the callback loads is never older than one collection.
        let mut sampler = tokio::time::interval(crate::telemetry::COLLECT_INTERVAL);
        sampler.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = token.cancelled() => break,
                _tick = sampler.tick() => {
                    afd_observability::producers::memory::resident_observed(
                        crate::telemetry::resident_bytes(),
                    );
                }
            }
        }
        // The last thing that happens to telemetry. Every signal the process
        // still holds is delivered here, before the pools it described are
        // dropped — which is the same ordering the analytics flush has, and
        // for the same reason.
        //
        // On the blocking pool, because `force_flush` parks the thread that
        // calls it: the span and log processors wait up to five seconds each,
        // and the metric reader's wait has NO timeout at all — it returns when
        // a collect-and-export finishes, bounded only by the configured OTLP
        // timeout. Run on a reactor thread that would block a worker for the
        // whole shutdown, and the supervisor's join deadline could not
        // interrupt it, because a task parked in a synchronous call has no
        // await point to cancel at.
        if tokio::task::spawn_blocking(move || exports.flush())
            .await
            .is_err()
        {
            tracing::warn!(
                event = "telemetry_flush_abandoned",
                "the shutdown flush did not run to completion"
            );
        }
    });
    Ok(())
}
