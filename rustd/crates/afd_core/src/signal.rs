//! Resolving when the process is asked to stop.
//!
//! Both signals are watched, because they arrive from different places and mean
//! the same thing here: SIGTERM from an orchestrator — Fly for `agentsfleetd`,
//! systemd for `agentsfleet-runner` — and SIGINT from a terminal.
//!
//! One implementation both binaries call, behind the `signal` feature so the
//! value layer links no runtime for a crate that does not stop processes. It
//! lives in a library rather than beside either `main` so the degraded path —
//! one handler registered instead of two — is reachable from a suite.

/// Resolves when the process is asked to stop.
pub async fn shutdown() {
    #[cfg(unix)]
    stop_on(tokio::signal::unix::SignalKind::terminate()).await;

    #[cfg(not(unix))]
    drop(tokio::signal::ctrl_c().await);
}

/// Resolves on SIGINT, or on `terminate` when that kind could be registered.
///
/// `terminate` is a parameter rather than a constant because the only
/// interesting thing about registering a handler is what happens when
/// registration FAILS, and SIGTERM does not fail on demand. Tokio refuses the two kinds a process
/// cannot catch, so `SignalKind::from_raw(SIGKILL)` is a registration that
/// fails through the same public API production goes through — which is what
/// makes the fallback below testable rather than hoped-for.
#[cfg(unix)]
async fn stop_on(terminate: tokio::signal::unix::SignalKind) {
    let interrupt = tokio::signal::ctrl_c();

    let Ok(mut terminate) = tokio::signal::unix::signal(terminate) else {
        // One handler is better than none: SIGINT alone still stops the
        // process. Refusing to boot over a handler that would not register
        // would turn a degraded stop into no service at all.
        drop(interrupt.await);
        return;
    };

    tokio::select! {
        result = interrupt => drop(result),
        _ = terminate.recv() => {}
    }
}

/// [`stop_on`], for a suite that needs a registration which fails.
#[cfg(all(unix, feature = "test-util"))]
pub async fn stop_on_kind(terminate: tokio::signal::unix::SignalKind) {
    stop_on(terminate).await;
}
