//! The final shared receipt owner wipes its retained provider result before freeing it.

use af_wire::{
    decision::{DecisionAnswer, DecisionOutcome},
    task::{DecisionReceipt, DecisionReceiptInput},
};
use zeroize::Zeroize;

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

impl Zeroize for Receipt {
    fn zeroize(&mut self) {
        self.0.fencing_token.zeroize();
        if let DecisionReceipt::Provider { result } = &mut self.0.receipt {
            if let Some(model) = &mut result.reported_model {
                model.zeroize();
            }
            if let Some(usage) = &mut result.usage {
                usage.input_tokens.zeroize();
                usage.output_tokens.zeroize();
                usage.cached_input_tokens.zeroize();
                usage.cache_write_tokens.zeroize();
            }
            if let DecisionOutcome::Answered { answers } = &mut result.result {
                // Map keys must leave the ordering structure before being overwritten.
                for (mut key, mut answer) in std::mem::take(answers) {
                    key.zeroize();
                    wipe_answer(&mut answer);
                }
            }
        }
    }
}

impl Drop for Receipt {
    fn drop(&mut self) {
        self.zeroize();
    }
}

fn wipe_answer(answer: &mut DecisionAnswer) {
    match answer {
        DecisionAnswer::Noul { noul } => noul.zeroize(),
        DecisionAnswer::Choice {
            choice,
            confidence,
            probabilities,
        } => {
            choice.zeroize();
            if let Some(confidence) = confidence {
                confidence.zeroize();
            }
            for (mut key, mut probability) in std::mem::take(probabilities) {
                key.zeroize();
                probability.zeroize();
            }
        }
        DecisionAnswer::Score {
            score,
            confidence,
            probabilities,
        } => {
            score.zeroize();
            if let Some(confidence) = confidence {
                confidence.zeroize();
            }
            for (mut key, mut probability) in std::mem::take(probabilities) {
                key.zeroize();
                probability.zeroize();
            }
        }
    }
}

#[cfg(test)]
mod tests;
