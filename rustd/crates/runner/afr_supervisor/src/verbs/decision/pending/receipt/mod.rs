//! The shared receipt owner keeps retained provider results out of debug output.
//!
//! Receipts hold fences, usage counts, a model name and answer probabilities: no credential or
//! tenant text, so they are not wiped (Indy, Nov 10, 2026).

use af_wire::task::DecisionReceiptInput;

pub(in super::super) struct Receipt(DecisionReceiptInput);

impl Receipt {
    pub(super) const fn new(input: DecisionReceiptInput) -> Self {
        Self(input)
    }
}

impl std::ops::Deref for Receipt {
    type Target = DecisionReceiptInput;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Debug for Receipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Receipt").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
