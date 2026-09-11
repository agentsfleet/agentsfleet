//! Where telemetry is switched on, and what the two boot-owned gauges read.
//!
//! Split from `serve` because boot's job is to open things and this is one
//! thing it opens: the export pipelines, the sampler that keeps the resident
//! gauge fresh, and the flush that runs when the process is asked to stop.

use std::sync::Arc;
use std::time::Duration;

use afd_api::Admission;
use afd_observability::producers::GaugeSources;

use crate::error::BootFailure;
use crate::preflight::BootConfig;
use crate::supervisor::{JOIN_TIMEOUT, Supervisor};

/// How long shutdown spends delivering what telemetry still holds.
///
/// The flush is NOT bounded by its own parts. `Exports::flush` walks four
/// providers in sequence; the span and log processors wait five seconds each,
/// and both metric readers wait on a channel with `recv()` and no timeout at
/// all — `PeriodicReader::force_flush` takes as long as a collect-and-export
/// takes, which the operator-set OTLP timeout is the only bound on. With the
/// 10 s default that is 5 + 10 + 10 + 5 against a ten-second join budget, and
/// a deployment that set a longer timeout makes it worse.
///
/// So the budget is here, strictly under [`JOIN_TIMEOUT`], and the point of
/// the gap is that the supervisor sees this task FINISH rather than abandon
/// it: an abandoned task is reported as a failed shutdown and tells an
/// operator nothing about which signal was lost. Bounded, the join is clean
/// and the warning below names what did not make it out.
const FLUSH_BUDGET: Duration = Duration::from_secs(8);

/// The budget only works while it is under the join deadline it is protecting.
const _: () = assert!(
    FLUSH_BUDGET.as_secs() < JOIN_TIMEOUT.as_secs(),
    "the shutdown flush budget must leave the supervisor room to join it"
);

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

/// What [`attach_exports`] built, waiting for the gauges boot has not opened yet.
///
/// Carries its own config rather than making the second half go back to
/// `BootConfig` for it. A `Prepared` exists only where an endpoint was
/// configured, so re-reading `config.otlp()` there would be an `Option` that
/// cannot be `None` — an invariant expressed as a panic instead of as a field.
pub(crate) struct Prepared {
    exports: crate::telemetry::Exports,
    instruments: afd_observability::metrics::instrument::Instruments,
    otlp: crate::preflight::OtlpConfig,
}

/// Builds the export pipelines and points this process's records at them,
/// BEFORE anything worth exporting is logged.
///
/// This is the first half, and its position in boot is the whole point. The
/// subscriber installed in `main` carries an empty reload slot, and every
/// record emitted before that slot is filled reaches stderr and nothing else.
/// The pool's own boot records — `pool_initialized`, `pool_warmed`,
/// `pool_warm_incomplete` — are emitted while `open_runtime` opens the
/// database, so a slot filled after it never carried them. It did not, and an
/// operator reading the collector for "did this instance warm its pool?" found
/// nothing and could conclude nothing.
///
/// The gauge producers cannot move here — they read the admission semaphore
/// and the live stream handle, which `open_runtime` creates — so they stay in
/// [`open_telemetry`], which takes what this returns.
///
/// `Ok(None)` is a deployment that configured no collector: it boots, serves,
/// and exports nothing, which is every developer's environment and most tests.
///
/// # Errors
/// A configuration the exporter will not accept, or a census the instrument
/// layer refuses. Both refuse boot, for the reason `telemetry::install` gives.
pub(crate) fn attach_exports(config: &BootConfig) -> Result<Option<Prepared>, BootFailure> {
    let Some(otlp) = config.otlp() else {
        // The Zig daemon's own event name and reason field, kept: a dashboard
        // or an alert matching on this line matches it from either binary.
        tracing::info!(
            reason = "no endpoint configured",
            event = "startup_otel_disabled",
            "telemetry is not exporting"
        );
        return Ok(None);
    };

    let (exports, instruments) = crate::telemetry::install(otlp)?;
    if let Some(signals) = crate::logs::signals() {
        let attached = signals.attach(&exports);
        tracing::debug!(attached, event = "telemetry_layers_attached");
    }
    Ok(Some(Prepared {
        exports,
        instruments,
        otlp: otlp.clone(),
    }))
}

/// Installs the gauge producers and supervises the flush, where a collector is
/// configured.
///
/// The second half of what [`attach_exports`] began. The task is spawned only
/// in the configured case, which is why the daemon's inventory carries it
/// conditionally and `integration_serve.rs` says so.
///
/// # Errors
/// A census the producer layer refuses, which is a defect that would otherwise
/// present as a collector receiving no gauges.
pub(crate) fn open_telemetry(
    prepared: Option<Prepared>,
    supervisor: &mut Supervisor,
    sources: &GaugeSources,
) -> Result<(), BootFailure> {
    let Some(Prepared {
        exports,
        instruments,
        otlp,
    }) = prepared
    else {
        return Ok(());
    };
    // Announced here rather than beside the pipelines, because the count it
    // reports is the claimed instrument set, and nothing is claimed until the
    // producers are.
    let installed = afd_observability::producers::install(&instruments, sources)?;
    crate::telemetry::announce(&otlp, installed, &instruments);

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
        // calls it, and a parked reactor worker is one the whole shutdown
        // waits behind. Under a budget as well, because moving the parking off
        // the reactor does not shorten it: see `FLUSH_BUDGET`.
        match tokio::time::timeout(
            FLUSH_BUDGET,
            tokio::task::spawn_blocking(move || exports.flush()),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(_panicked)) => tracing::warn!(
                event = "telemetry_flush_abandoned",
                "the shutdown flush did not run to completion"
            ),
            Err(_elapsed) => tracing::warn!(
                budget_ms = FLUSH_BUDGET.as_millis(),
                event = "telemetry_flush_timed_out",
                "the shutdown flush outran its budget — some telemetry was not delivered"
            ),
        }
    });
    Ok(())
}
