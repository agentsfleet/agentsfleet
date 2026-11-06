//! What the billing lifecycle did: funds checked, lines settled, tokens left
//! unpriced, and what is still owed.
//!
//! Recorded at one choke point, the biller every money path holds (billing
//! §11), so a check or a settlement is counted once whichever product path or
//! relay pass made it. The two gauges are the billing relay's: it is the one
//! reader that already scans the unsettled lines.

use crate::metrics::family::{CounterKind, Declared, GaugeKind};

/// Funds checks by their answer.
///
/// Labels: `work_type,outcome`.
pub const BILLING_FUNDS_CHECK_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_billing_funds_check_total");

/// Settlements by whether they landed.
///
/// Labels: `work_type,outcome`.
pub const BILLING_SETTLE_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_billing_settle_total");

/// Tokens a settlement carried with no published price.
///
/// Labels: `gen_ai.provider.name,gen_ai.request.model`.
pub const BILLING_UNPRICED_TOKENS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_billing_unpriced_tokens_total");

/// Work/provider/model/posture groups whose unsettled lines the relay settled.
///
/// Labels: `work_type`.
pub const BILLING_SWEEPER_RESETTLED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_billing_sweeper_resettled_total");

/// Nanos logged and not yet settled, as the last relay pass read them.
///
/// Labels: `work_type`.
pub const BILLING_UNSETTLED_NANOS: Declared<GaugeKind> =
    Declared::new("agentsfleet_billing_unsettled_nanos");

/// Age of the oldest unsettled line, as the last relay pass read it.
pub const BILLING_UNSETTLED_OLDEST_SECONDS: Declared<GaugeKind> =
    Declared::new("agentsfleet_billing_unsettled_oldest_seconds");
