//! Who a process says it is, on every signal it sends.
//!
//! One resource for all three signals, byte-identical, because a log, a span
//! and a metric from one process describe the same service or they cannot be
//! correlated at the other end.

use afd_observability::semconv;
use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;

/// An operator-supplied replica identity.
pub const INSTANCE_ID_KNOB: &str = "OTEL_SERVICE_INSTANCE_ID";

/// The platform's own machine identity, used when the operator supplies none.
pub const MACHINE_ID_KNOB: &str = "FLY_MACHINE_ID";

/// Which process is exporting: its service name and the build it is.
///
/// The name doubles as the instrumentation scope every instrument is built
/// under, so a span and a metric from one binary name it the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Service {
    name: &'static str,
    version: &'static str,
}

impl Service {
    /// The service `name`, at `version`.
    #[must_use]
    pub const fn new(name: &'static str, version: &'static str) -> Self {
        Self { name, version }
    }

    /// The service name, and the scope its instruments are built under.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// This process, as every signal describes it.
    pub(crate) fn describe(&self) -> Resource {
        let mut builder = Resource::builder()
            .with_service_name(self.name)
            .with_attribute(KeyValue::new(
                semconv::RESOURCE_SERVICE_NAMESPACE,
                semconv::SERVICE_NAMESPACE,
            ))
            .with_attribute(KeyValue::new(
                semconv::RESOURCE_SERVICE_VERSION,
                self.version,
            ));
        if let Some(instance) = instance_id() {
            builder = builder.with_attribute(KeyValue::new(
                semconv::RESOURCE_SERVICE_INSTANCE_ID,
                instance,
            ));
        }
        builder.build()
    }
}

/// Which replica this is, when something can say so truthfully.
///
/// Read from the process environment directly rather than through a knob
/// reader, and that is the one place in this crate where that is right: it is
/// not a knob an operator sets for a binary, it is an identity the platform
/// injects, and a boot that refused over a missing one would refuse every
/// deployment that is not on that platform.
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

#[cfg(test)]
mod tests {
    use afd_observability::semconv;
    use opentelemetry::Key;

    use super::Service;

    /// The resource names the service, its namespace and its version, and
    /// fabricates no instance id.
    #[test]
    fn the_resource_names_the_service_and_invents_no_instance() {
        let described = Service::new("a-service", "1.2.3").describe();

        let read = |key: &'static str| {
            described
                .get(&Key::from_static_str(key))
                .map(|value| value.to_string())
        };
        assert_eq!(read("service.name").as_deref(), Some("a-service"));
        assert_eq!(
            read(semconv::RESOURCE_SERVICE_NAMESPACE).as_deref(),
            Some(semconv::SERVICE_NAMESPACE)
        );
        assert_eq!(
            read(semconv::RESOURCE_SERVICE_VERSION).as_deref(),
            Some("1.2.3")
        );
        // A test process carries neither knob, so no identity is invented.
        if std::env::var(super::INSTANCE_ID_KNOB).is_err()
            && std::env::var(super::MACHINE_ID_KNOB).is_err()
        {
            assert_eq!(read(semconv::RESOURCE_SERVICE_INSTANCE_ID), None);
        }
    }
}
