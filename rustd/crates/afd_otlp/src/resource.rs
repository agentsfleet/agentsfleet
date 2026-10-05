//! Who a process says it is, on every signal it sends.
//!
//! One resource for all three signals, byte-identical, because a log, a span
//! and a metric from one process describe the same service or they cannot be
//! correlated at the other end.

use afd_core::env::{EnvSource, ProcessEnv};
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
        self.described_from(&ProcessEnv)
    }

    /// This process, its replica identity read from `env`.
    fn described_from<E: EnvSource + ?Sized>(&self, env: &E) -> Resource {
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
        if let Some(instance) = instance_id(env) {
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
/// Read straight from the environment rather than through a binary's knob
/// reader: it is not a knob an operator sets for a binary, it is an identity
/// the platform injects, and a boot that refused over a missing one would
/// refuse every deployment that is not on that platform.
///
/// Absent by default and deliberately. A FABRICATED instance id multiplies
/// every series by the replica count without being trustworthy; an absent one
/// leaves replicas publishing cumulative sums under one series identity, which
/// a store reads as counter resets. Only a real identity is worth having, so
/// only a real one is sent.
fn instance_id<E: EnvSource + ?Sized>(env: &E) -> Option<String> {
    [INSTANCE_ID_KNOB, MACHINE_ID_KNOB]
        .into_iter()
        .filter_map(|knob| env.get(knob))
        .map(|value| value.trim().to_owned())
        .find(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use afd_core::env::MapEnv;
    use afd_observability::semconv;
    use opentelemetry::Key;
    use opentelemetry_sdk::Resource;

    use super::{INSTANCE_ID_KNOB, MACHINE_ID_KNOB, Service};

    /// The service a test describes.
    const SERVICE: Service = Service::new("a-service", "1.2.3");

    /// The value `resource` carries under `key`.
    fn read(resource: &Resource, key: &'static str) -> Option<String> {
        resource
            .get(&Key::from_static_str(key))
            .map(|value| value.to_string())
    }

    /// The resource names the service, its namespace and its version, and
    /// fabricates no instance id where the platform supplies none.
    #[test]
    fn the_resource_names_the_service_and_invents_no_instance() {
        let described = SERVICE.described_from(&MapEnv::from_pairs([(INSTANCE_ID_KNOB, "  ")]));

        assert_eq!(
            read(&described, "service.name").as_deref(),
            Some("a-service")
        );
        assert_eq!(
            read(&described, semconv::RESOURCE_SERVICE_NAMESPACE).as_deref(),
            Some(semconv::SERVICE_NAMESPACE)
        );
        assert_eq!(
            read(&described, semconv::RESOURCE_SERVICE_VERSION).as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            read(&described, semconv::RESOURCE_SERVICE_INSTANCE_ID),
            None,
            "a blank identity is no identity"
        );
        assert_eq!(SERVICE.name(), "a-service");
    }

    /// The operator's instance id outranks the platform's machine id, which
    /// stands in when the operator gives none; both are trimmed.
    #[test]
    fn the_operators_instance_outranks_the_platforms_machine() {
        let both =
            MapEnv::from_pairs([(INSTANCE_ID_KNOB, " replica-a "), (MACHINE_ID_KNOB, "m-1")]);
        let machine = MapEnv::from_pairs([(INSTANCE_ID_KNOB, ""), (MACHINE_ID_KNOB, "m-1")]);

        assert_eq!(
            read(
                &SERVICE.described_from(&both),
                semconv::RESOURCE_SERVICE_INSTANCE_ID
            )
            .as_deref(),
            Some("replica-a")
        );
        assert_eq!(
            read(
                &SERVICE.described_from(&machine),
                semconv::RESOURCE_SERVICE_INSTANCE_ID
            )
            .as_deref(),
            Some("m-1")
        );
    }

    /// The process environment is what production reads.
    #[test]
    fn the_production_resource_reads_the_process() {
        let described = SERVICE.describe();
        assert_eq!(
            read(&described, "service.name").as_deref(),
            Some("a-service")
        );
    }
}
