//! The bus gap and outbound claim producers record what they are handed,
//! under every label each accepts, on a real SDK reader.

use af_observability::metrics::declared::{bus, outbound};
use af_observability::metrics::label::bus::GapCause;
use af_observability::metrics::label::outbound::ClaimVia;
use af_observability::producers;
use af_observability::semconv::{LABEL_CAUSE, LABEL_VIA};
use af_observability::test_util::Capture;

/// Subscriptions one recorded gap told.
const TOLD: u64 = 3;
/// Rows one recorded claim took.
const CLAIMED: u64 = 2;

#[test]
fn every_bus_gap_cause_records_its_subscriptions() {
    let captured = Capture::install();
    for cause in GapCause::ALL.iter().copied() {
        let labels = [(LABEL_CAUSE, cause.as_str())];
        let before = captured.sum(bus::GAPS.wire_name(), &labels);

        producers::bus::gaps(cause, TOLD);

        assert_eq!(
            captured.sum(bus::GAPS.wire_name(), &labels) - before,
            TOLD,
            "a {} gap adds the subscriptions it told",
            cause.as_str()
        );
    }
}

#[test]
fn every_outbound_claim_trigger_records_its_rows() {
    let captured = Capture::install();
    for via in ClaimVia::ALL.iter().copied() {
        let labels = [(LABEL_VIA, via.as_str())];
        let before = captured.sum(outbound::CLAIMS.wire_name(), &labels);

        producers::outbound::claims(via, CLAIMED);

        assert_eq!(
            captured.sum(outbound::CLAIMS.wire_name(), &labels) - before,
            CLAIMED,
            "a {} claim adds the rows it took",
            via.as_str()
        );
    }
}
