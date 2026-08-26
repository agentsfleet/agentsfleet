//! What a gate's own failure decides, as data rather than as control flow.
//!
//! # Named for the fault, not for the posture
//!
//! `Posture` is already taken, and by the money path: [`crate::money::Posture`]
//! is who supplies the provider key, which is the spelling
//! `billing.usage_ledger.posture` uses and therefore the one that keeps its
//! name. A module called `posture` beside it meant [`super`] imported
//! `posture::PAYER` and `Posture` on adjacent lines for two unrelated
//! questions — who pays, and what a dead datastore means.
//!
//! `balanceCoversEstimate` contains three separate `catch return true` — on the
//! acquire, on the wallet load, and on the rate lookup. Each is a fail-open
//! decision a reviewer has to notice individually, and a fourth statement added
//! later inherits nothing from the three above it.
//!
//! Here the posture is a field on a [`Gate`] value, declared beside the gate's
//! name, and [`Gate::absorb`] is the only code that acts on it. Adding a gate
//! means stating a posture; changing a posture means editing one line that
//! reads like the sentence it enforces.

use afd_core::error_code;

use crate::error::Error;
use crate::lease::admit::{Admission, Transient};

/// What a gate's own failure decides.
///
/// Two arms and not three: a DATASTORE fault never ends an event permanently.
/// Permanence comes from verdicts — an exhausted wallet, a breached ceiling, a
/// workspace naming no tenant — and those arrive as `Ok`, not as `Err`. That
/// split is RULE ECL held at the type level rather than in a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OnFault {
    /// Admit the event. The two MONEY gates take this posture: a metering
    /// outage must not halt every fleet on the platform, and a gate stricter
    /// than the platform's own credit pool would be an inconsistent guarantee.
    Admit,
    /// Answer no-work. The delivery stays leasable and the next poll retries.
    Retry,
}

/// One gate, and what its failure means.
#[derive(Debug, Clone, Copy)]
pub(super) struct Gate {
    /// The `event` a fault at this gate logs under.
    pub(super) event: &'static str,
    /// What a fault here decides.
    pub(super) on_fault: OnFault,
}

/// The tenant's credit pool. Fails OPEN.
pub(super) const BALANCE: Gate = Gate {
    event: "lease_balance_unavailable",
    on_fault: OnFault::Admit,
};

/// The fleet's own declared ceiling. Fails OPEN, mirroring [`BALANCE`] — a
/// budget gate stricter than the credit gate above it would be an inconsistent
/// guarantee, which is the reasoning `budget.zig` gives for its own posture.
pub(super) const BUDGET: Gate = Gate {
    event: "lease_budget_unavailable",
    on_fault: OnFault::Admit,
};

/// Recording the receive charge. Fails to RETRY: an uncharged run must not
/// proceed, but nothing here is terminal — the next poll charges it.
pub(super) const RECEIPT: Gate = Gate {
    event: "lease_receive_debit_unavailable",
    on_fault: OnFault::Retry,
};

/// Resolving who pays. Fails to RETRY.
pub(super) const PAYER: Gate = Gate {
    event: "lease_tenant_lookup_failed",
    on_fault: OnFault::Retry,
};

impl Gate {
    /// Apply this gate's declared posture to a fault, logging it once.
    ///
    /// `None` means the pass continues — which for [`OnFault::Admit`] IS the
    /// fail-open decision, taken here rather than by a `catch` returning a bare
    /// `true` eight frames down.
    ///
    /// A caller at a [`OnFault::Retry`] gate knows this answers `Some` and
    /// spells that `absorb(&fault).ok_or(fault)`: one expression, and if the
    /// posture is ever changed to `Admit` the gate propagates the error instead
    /// of silently admitting — which is the right way for that edit to fail.
    pub(super) fn absorb(self, fault: &Error) -> Option<Admission> {
        // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
        // scores the dead copy.
        let code = error_code::INTERNAL_DB_QUERY.as_str();
        let reason = fault.to_string();
        let event = self.event;
        match self.on_fault {
            OnFault::Admit => {
                tracing::warn!(
                    error_code = code,
                    event,
                    reason,
                    "a money gate could not be read; the event is admitted rather than \
                     halting every fleet on the platform"
                );
                None
            }
            OnFault::Retry => {
                tracing::warn!(
                    error_code = code,
                    event,
                    reason,
                    "the pass ended early; the delivery stays leasable and the next poll retries"
                );
                Some(Admission::Retry(Transient { at: event }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BALANCE, BUDGET, OnFault, PAYER, RECEIPT};
    use std::collections::BTreeSet;

    #[test]
    fn the_two_money_gates_fail_open_and_the_two_write_gates_do_not() {
        // The asymmetry stated as a test rather than left to a reader
        // collecting `catch` sites: a metering outage admits, an unresolvable
        // payer or an unrecorded charge does not.
        assert_eq!(BALANCE.on_fault, OnFault::Admit);
        assert_eq!(BUDGET.on_fault, OnFault::Admit);
        assert_eq!(PAYER.on_fault, OnFault::Retry);
        assert_eq!(RECEIPT.on_fault, OnFault::Retry);
    }

    #[test]
    fn every_gate_logs_under_a_distinct_event() {
        // Two gates sharing an event name would make a dashboard unable to say
        // WHICH read failed, which is the only thing the line is for.
        let events = [BALANCE.event, BUDGET.event, PAYER.event, RECEIPT.event];
        let distinct: BTreeSet<_> = events.iter().collect();
        assert_eq!(
            distinct.len(),
            events.len(),
            "two gates share an event name: {events:?}"
        );
        assert!(events.iter().all(|event| !event.is_empty()));
    }
}
