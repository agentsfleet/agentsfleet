//! What the billing lifecycle did: funds checked and tokens left unpriced.
//!
//! Recorded at one choke point, the biller every money path holds, so a check
//! is counted once whichever product path made it.

use crate::metrics::family::{CounterKind, Declared};

/// Funds checks by their answer.
///
/// Labels: `work_type,outcome`.
pub const BILLING_FUNDS_CHECK_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_billing_funds_check_total");

/// Tokens a settlement carried with no published price.
///
/// Labels: `gen_ai.provider.name,gen_ai.request.model`.
pub const BILLING_UNPRICED_TOKENS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_billing_unpriced_tokens_total");
