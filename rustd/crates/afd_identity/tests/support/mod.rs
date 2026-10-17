//! Shared scaffolding for this crate's test targets.
//!
//! Each test target compiles this module separately, so a helper only some
//! targets reach for reads as dead in the others. That is a property of how
//! Cargo builds integration tests, not unused code.
#![allow(
    dead_code,
    reason = "shared test support is compiled into every including target; not all of them use all of it"
)]

pub(crate) mod capability;
pub(crate) mod signing;

/// One millisecond, the step that puts a clock just past a window's edge.
pub(crate) const ONE_MS: std::time::Duration = std::time::Duration::from_millis(1);

/// Where every capability suite's clock starts.
///
/// Arbitrary, and identical across every window test on purpose: the
/// assertions are about elapsed time from a common origin, so one name is what
/// keeps them comparable.
pub(crate) const CLOCK_ORIGIN: jiff::Timestamp = jiff::Timestamp::constant(1_000, 0);

/// Installs a subscriber so event macros actually run.
///
/// `tracing::warn!` asks whether its callsite is enabled BEFORE it evaluates
/// the fields inside it, so with no subscriber every field expression in every
/// diagnostic is skipped — the failure path runs and the line reporting it does
/// not. Output goes to a sink; the point is evaluation, not reading.
///
/// The same helper `afd_db` and `afd_dragonfly` carry, for the same reason.
pub(crate) fn install_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}
