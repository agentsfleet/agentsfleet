//! Who this process says it is, on every signal it sends.
//!
//! One resource for all three signals, byte-identical, because a log, a span
//! and a metric from this process describe the same service or they cannot be
//! correlated at the other end.

use afd_observability::semconv;
use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;

/// An operator-supplied replica identity.
pub const INSTANCE_ID_KNOB: &str = "OTEL_SERVICE_INSTANCE_ID";

/// The platform's own machine identity, used when the operator supplies none.
pub const MACHINE_ID_KNOB: &str = "FLY_MACHINE_ID";

/// This process, as every signal describes it.
pub(super) fn describe() -> Resource {
    let mut builder = Resource::builder()
        .with_service_name(semconv::SCOPE_NAME)
        .with_attribute(KeyValue::new(
            semconv::RESOURCE_SERVICE_NAMESPACE,
            semconv::SERVICE_NAMESPACE,
        ))
        .with_attribute(KeyValue::new(
            semconv::RESOURCE_SERVICE_VERSION,
            env!("CARGO_PKG_VERSION"),
        ));
    if let Some(instance) = instance_id() {
        builder = builder.with_attribute(KeyValue::new(
            semconv::RESOURCE_SERVICE_INSTANCE_ID,
            instance,
        ));
    }
    builder.build()
}

/// Which replica this is, when something can say so truthfully.
///
/// Read from the process environment directly rather than through `preflight`,
/// and that is the one place in this daemon where that is right: it is not a
/// knob an operator sets for this daemon, it is an identity the platform
/// injects, and a preflight that refused boot over a missing one would refuse
/// every deployment that is not on that platform.
///
/// Absent by default and deliberately. A FABRICATED instance id multiplies
/// every series by the replica count without being trustworthy; an absent one
/// leaves replicas publishing cumulative sums under one series identity, which
/// a store reads as counter resets. Only a real identity is worth having, so
/// only a real one is sent.
fn instance_id() -> Option<String> {
    [INSTANCE_ID_KNOB, MACHINE_ID_KNOB]
        .into_iter()
        .filter_map(|knob| std::env::var(knob).ok())
        .map(|value| value.trim().to_owned())
        .find(|value| !value.is_empty())
}

/// How many bytes of memory this process is holding, where that is readable.
///
/// Linux only, and absent everywhere else rather than approximated. `statm`
/// answers in pages and its second field is the resident set; a developer's
/// macOS box has no such file, and a number invented for it would be a
/// measurement nobody took reported as one that was.
pub(crate) fn resident_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages.saturating_mul(PAGE_SIZE))
}

/// The page size the reading above is multiplied by.
///
/// Stated rather than probed: `sysconf` needs a libc dependency this crate
/// carries for nothing else, and the deployment this reading is taken on is a
/// 4 KiB-page Linux. On a host with larger pages the number would under-report
/// — which is why the constant is named and not inlined, so the assumption is
/// one line to find and one line to change.
const PAGE_SIZE: u64 = 4_096;
