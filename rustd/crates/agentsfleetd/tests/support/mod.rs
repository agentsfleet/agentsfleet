//! Shared scaffolding for the daemon's suites.

/// Installs a subscriber so event macros actually run.
///
/// `tracing::error!` asks whether its callsite is enabled BEFORE evaluating the
/// fields inside it, so with no subscriber a diagnostic's fields never run —
/// the failure path executes and the line reporting it does not. Output goes to
/// a sink; the point is evaluation, not reading.
pub(crate) fn install_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        let _previous = tracing::subscriber::set_global_default(subscriber);
    });
}
