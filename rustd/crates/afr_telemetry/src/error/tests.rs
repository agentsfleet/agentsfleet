//! Each failure keeps its kind across the lift from the layers beneath.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::error::Error as _;

use afd_core::error_code;
use afd_observability::metrics::registry::Registry;
use afd_otlp::{Builder, OTEL_ENDPOINT_KNOB, OtlpConfig, Refused, Service};

use super::Error;

/// A census of one counter whose ceiling the SDK refuses.
const ZERO_CEILING: &str = "name\tkind\tnumber\tunit\ttemporality\tlabels\tbounds\tpolicy\tlive_read\tcategory\twatch_for\n\
                            a.family\tcounter\tu64\t1\tcumulative\t-\t-\tfixed:0\tno\ttraffic\tnothing\n";

/// A transport failure that is not a knob stays the transport's: no knob,
/// the transport's own code, and its cause kept.
#[test]
fn a_transport_failure_keeps_its_code_and_names_no_knob() {
    let config =
        OtlpConfig::new("http://127.0.0.1:1", OTEL_ENDPOINT_KNOB).expect("an absolute URL");
    let registry = Registry::read(ZERO_CEILING).expect("a zero ceiling reads");
    let transport = Builder::new(&config, Service::new("a-test", "0.0.0"), registry)
        .install()
        .expect_err("the SDK refuses a zero ceiling");

    let lifted = Error::from(transport);

    assert_eq!(lifted.knob(), None);
    assert_eq!(lifted.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(
        lifted.source().is_some(),
        "the transport's failure is the cause"
    );
}

/// A knob the transport refused is still a knob once lifted.
#[test]
fn a_knob_the_transport_refused_stays_a_knob() {
    let refused = afd_otlp::Error::from(Refused {
        knob: OTEL_ENDPOINT_KNOB,
        why: "a sentence",
    });

    let lifted = Error::from(refused);

    assert_eq!(lifted.knob(), Some(OTEL_ENDPOINT_KNOB));
    assert_eq!(lifted.code(), error_code::STARTUP_ENV_CHECK);
}

/// A census the instrument layer refuses is the build's defect.
#[test]
fn a_refused_census_is_a_defect_with_no_knob() {
    let census = Registry::read(&ZERO_CEILING.replace("counter", "bogus")).map(drop);
    let contract = census.expect_err("a census declaring a kind nobody spelled does not read");

    let lifted = Error::from(contract);

    assert_eq!(lifted.knob(), None);
    assert_eq!(lifted.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(lifted.to_string().contains("contract"), "{lifted}");
}
