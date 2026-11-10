//! Outbound answers claimed from the obligation ledger.
use crate::metrics::family::{CounterKind, Declared};

/// Obligation rows a claim took, by what triggered the claim.
pub const CLAIMS: Declared<CounterKind> = Declared::new("agentsfleet_outbound_claims_total");
