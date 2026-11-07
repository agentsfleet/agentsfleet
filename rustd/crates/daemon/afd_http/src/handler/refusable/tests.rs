//! The refusal adapters' code and class mapping, one sampler per crate.
//!
//! Lifted out of `refusable.rs` at the file cap, which is the first cut the
//! length rule asks for: inline tests are what a coverage instrument counts
//! as product, and moving them frees the most lines for the least risk.

use afd_core::error_code::INTERNAL_DB_UNAVAILABLE;

/// Every adapter here is four one-line delegations, and that is exactly why
/// they are worth grading: a `code()` that called the wrong inherent method
/// or an `is_datastore_unavailable` that compared against the wrong code
/// compiles, reads correctly, and turns an outage into a 500 a caller will
/// never retry. The samplers come from the crates that RAISE each kind, so
/// this cannot drift from them the way a hand-built error would.
macro_rules! grades_every_kind {
    ($name:ident, $sample:path) => {
        #[test]
        fn $name() {
            let kinds = $sample();
            assert!(!kinds.is_empty(), "the sampler carries at least one kind");

            for (label, error) in &kinds {
                let rendered = error.reason();
                assert!(
                    rendered.contains(error.code().as_str()),
                    "{label}'s reason must carry its code, because it is the \
                     only field an operator can grep the log by: {rendered}"
                );
                assert!(
                    !Refusable::detail(error).is_empty(),
                    "{label} answers a client with an empty sentence"
                );
                assert_eq!(
                    Refusable::is_datastore_unavailable(error),
                    Refusable::code(error) == INTERNAL_DB_UNAVAILABLE,
                    "{label}: an outage is exactly the unavailable code, and \
                     a disagreement here is a 503 answered as a 500 or the \
                     reverse — one is retried and the other is not"
                );
            }
        }
    };
}

use super::Refusable;

grades_every_kind!(
    the_cron_adapter_grades_every_kind_that_crate_raises,
    afd_cron::error::one_of_each_kind
);
grades_every_kind!(
    the_connector_adapter_grades_every_kind_that_crate_raises,
    afd_connector::error::one_of_each_kind
);
grades_every_kind!(
    the_ingress_adapter_grades_every_kind_that_crate_raises,
    afd_ingress::error::one_of_each_kind
);
