//! What the billing lifecycle records: funds checks, settlements and unpriced
//! tokens.
//!
//! The counters are recorded by the one biller wrapper every money path holds,
//! so a check or a settlement counts once whoever made it.

use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;

use crate::error::Result;
use crate::metrics::declared::billing as declared;
use crate::metrics::instrument::Instruments;
use crate::metrics::label::billing::{FundsOutcome, SettleOutcome, WorkType};
use crate::producers::installed;
use crate::semconv;

/// The billing counters, claimed once.
#[derive(Debug)]
pub struct Handles {
    funds: Counter<u64>,
    settle: Counter<u64>,
    unpriced: Counter<u64>,
}

impl Handles {
    pub(super) fn claim(instruments: &Instruments) -> Result<Self> {
        Ok(Self {
            funds: instruments.counter_u64(&declared::BILLING_FUNDS_CHECK_TOTAL)?,
            settle: instruments.counter_u64(&declared::BILLING_SETTLE_TOTAL)?,
            unpriced: instruments.counter_u64(&declared::BILLING_UNPRICED_TOKENS_TOTAL)?,
        })
    }
}

/// One funds check's answer.
pub fn funds_checked(work: WorkType, outcome: FundsOutcome) {
    if let Some(producers) = installed() {
        producers.billing.funds.add(
            1,
            &[
                KeyValue::new(semconv::LABEL_WORK_TYPE, work.as_str()),
                KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str()),
            ],
        );
    }
}

/// One settlement's outcome.
pub fn settled(work: WorkType, outcome: SettleOutcome) {
    if let Some(producers) = installed() {
        producers.billing.settle.add(
            1,
            &[
                KeyValue::new(semconv::LABEL_WORK_TYPE, work.as_str()),
                KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str()),
            ],
        );
    }
}

/// Tokens a settlement carried with no published price.
pub fn unpriced_tokens(provider: &str, model: &str, tokens: u64) {
    if tokens == 0 {
        return;
    }
    if let Some(producers) = installed() {
        producers.billing.unpriced.add(
            tokens,
            &[
                KeyValue::new(semconv::ATTR_PROVIDER_NAME, provider.to_owned()),
                KeyValue::new(semconv::ATTR_REQUEST_MODEL, model.to_owned()),
            ],
        );
    }
}
