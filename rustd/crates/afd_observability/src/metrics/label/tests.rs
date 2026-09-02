//! The census ceilings admit the label sets this build actually writes.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use crate::metrics::declared::{fleet, http, library};
use crate::metrics::label::cost::{ChargeClass, TokenType};
use crate::metrics::label::fleet as fleet_labels;
use crate::metrics::label::http as http_labels;
use crate::metrics::label::library as library_labels;
use crate::metrics::registry::{Policy, Registry};

/// Each labelled family and the number of series its closed sets can produce.
///
/// Written out rather than derived, because the pairing IS the claim: which
/// label sets a family carries is a fact about the producer, and a table that
/// derived it from the census would only be checking the census against
/// itself.
fn label_products() -> Vec<(&'static str, usize)> {
    vec![
        (
            http::HTTP_TRACE_SUPPRESSED_TOTAL.wire_name(),
            http_labels::TraceSuppression::ALL.len(),
        ),
        (
            http::OTLP_ENTRIES_DISCARDED_TOTAL.wire_name(),
            http_labels::Signal::ALL.len() * http_labels::DiscardReason::ALL.len(),
        ),
        (
            http::OTEL_ATTRIBUTE_OMITTED_TOTAL.wire_name(),
            http_labels::OmittedAttribute::ALL.len() * http_labels::OmissionReason::ALL.len(),
        ),
        (
            fleet::SIGNUP_FAILED_TOTAL.wire_name(),
            fleet_labels::SignupFailure::ALL.len(),
        ),
        (
            fleet::REPAIR_PROVIDER_RESULTS_TOTAL.wire_name(),
            fleet_labels::ProviderResult::ALL.len(),
        ),
        (
            fleet::REPAIR_CORRELATIONS_TOTAL.wire_name(),
            fleet_labels::Correlation::ALL.len(),
        ),
        (
            fleet::REPAIR_SYNTHETIC_EVENTS_TOTAL.wire_name(),
            fleet_labels::SyntheticEvent::ALL.len(),
        ),
        (
            fleet::REPAIR_VERIFIER_RUNS_TOTAL.wire_name(),
            fleet_labels::VerifierRun::ALL.len(),
        ),
        (
            library::LIBRARY_STAGE_DURATION_SECONDS_TOTAL.wire_name(),
            library_labels::Surface::ALL.len() * library_labels::Stage::ALL.len(),
        ),
        (
            library::LIBRARY_STAGE_OBSERVATIONS_TOTAL.wire_name(),
            library_labels::Surface::ALL.len() * library_labels::Stage::ALL.len(),
        ),
        (
            library::LIBRARY_READ_OUTCOME_TOTAL.wire_name(),
            library_labels::Surface::ALL.len() * library_labels::ReadOutcome::ALL.len(),
        ),
        (
            library::LIBRARY_POOL_RESULT_TOTAL.wire_name(),
            library_labels::PoolResult::ALL.len(),
        ),
        (
            library::LIBRARY_CACHE_OUTCOME_TOTAL.wire_name(),
            library_labels::CacheOutcome::ALL.len(),
        ),
        (
            library::LIBRARY_PAYLOAD_BYTES_TOTAL.wire_name(),
            library_labels::Surface::ALL.len(),
        ),
        (
            library::LIBRARY_RESULTS_TOTAL.wire_name(),
            library_labels::Surface::ALL.len(),
        ),
    ]
}

/// No family declares a ceiling below the number of series it can write.
///
/// The failure this prevents is silent and total. A ceiling under the real
/// count does not drop the excess label VALUES — the SDK folds them into
/// `otel.metric.overflow`, which `crate::runner` documents as a backstop that
/// must never fire, and the dashboard keeps drawing a line that is now the
/// wrong one. It was under the real count for six families until this test
/// existed.
#[test]
fn every_declared_ceiling_admits_its_label_product() {
    let registry = Registry::declared().expect("the compiled-in census reads");
    for (name, product) in label_products() {
        let family = registry
            .family(name)
            .expect("every family named here is declared");
        let Policy::Fixed { max_series } = family.policy else {
            unreachable!("`{name}` carries closed labels and no fixed ceiling");
        };
        assert!(
            max_series >= product,
            "the census admits {max_series} series for `{name}`, whose closed \
             label sets can write {product}"
        );
    }
}

/// The cost families' attribute sets are closed even though their budget is not.
///
/// They draw on the shared cost sub-budget rather than a fixed ceiling, so the
/// test above cannot cover them — but a token direction or a charge class
/// spelled two ways is still two series in the money dashboards.
#[test]
fn the_cost_attribute_sets_stay_closed() {
    assert_eq!(
        TokenType::ALL.len(),
        2,
        "input already includes the cached portion; a third direction would make the total wrong"
    );
    assert_eq!(ChargeClass::ALL.len(), 3);
}
