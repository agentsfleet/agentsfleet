//! One enabled tracing subscriber for Redis integration-test binaries.

/// Installs a subscriber so event fields execute on success and failure paths.
///
/// `tracing` checks whether a callsite is enabled before evaluating its fields.
/// A sink keeps test output quiet while still proving those expressions.
pub(crate) fn install_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        let _ignored = tracing::subscriber::set_global_default(subscriber);
    });
}
