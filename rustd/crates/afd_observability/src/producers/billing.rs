//! What the billing lifecycle records: funds checks, settlements, unpriced
//! tokens, and the relay's view of what is still owed.
//!
//! The counters are recorded by the one biller wrapper every money path holds,
//! so a check or a settlement counts once whoever made it; the gauges are
//! published by the billing relay, which already reads the rows they describe.

use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;

use crate::error::Result;
use crate::metrics::declared::billing as declared;
use crate::metrics::instrument::{Instruments, Reading};
use crate::metrics::label::billing::{FundsOutcome, SettleOutcome, WorkType};
use crate::metrics::observed::Observed;
use crate::producers::installed;
use crate::semconv;

/// Unsettled nanos per work type, in [`WorkType::ALL`] order.
static UNSETTLED: [Observed; 2] = [Observed::new(), Observed::new()];

/// Age of the oldest unsettled line.
static UNSETTLED_OLDEST: Observed = Observed::new();

/// The billing counters, claimed once.
#[derive(Debug)]
pub struct Handles {
    funds: Counter<u64>,
    settle: Counter<u64>,
    unpriced: Counter<u64>,
    resettled: Counter<u64>,
}

impl Handles {
    pub(super) fn claim(instruments: &Instruments) -> Result<Self> {
        instruments.gauge_u64(&declared::BILLING_UNSETTLED_NANOS, || {
            WorkType::ALL
                .iter()
                .zip(&UNSETTLED)
                .filter_map(|(work, cell)| {
                    cell.load().map(|value| Reading {
                        attributes: vec![KeyValue::new(semconv::LABEL_WORK_TYPE, work.as_str())],
                        value,
                    })
                })
                .collect()
        })?;
        instruments.gauge_u64(&declared::BILLING_UNSETTLED_OLDEST_SECONDS, || {
            UNSETTLED_OLDEST
                .load()
                .map(Reading::unlabelled)
                .into_iter()
                .collect()
        })?;
        Ok(Self {
            funds: instruments.counter_u64(&declared::BILLING_FUNDS_CHECK_TOTAL)?,
            settle: instruments.counter_u64(&declared::BILLING_SETTLE_TOTAL)?,
            unpriced: instruments.counter_u64(&declared::BILLING_UNPRICED_TOKENS_TOTAL)?,
            resettled: instruments.counter_u64(&declared::BILLING_SWEEPER_RESETTLED_TOTAL)?,
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

/// Works the billing relay settled.
pub fn sweeper_resettled(work: WorkType, works: u64) {
    if works == 0 {
        return;
    }
    if let Some(producers) = installed() {
        producers.billing.resettled.add(
            works,
            &[KeyValue::new(semconv::LABEL_WORK_TYPE, work.as_str())],
        );
    }
}

/// Publishes what the billing relay last read.
///
/// `None` withdraws every cell, for a pass whose read failed: a gap, never a
/// zero nobody measured.
pub fn owed_observed(owed: Option<Owed>) {
    let Some(owed) = owed else {
        for cell in &UNSETTLED {
            cell.withdraw();
        }
        UNSETTLED_OLDEST.withdraw();
        return;
    };
    for (cell, nanos) in UNSETTLED.iter().zip(owed.unsettled_nanos) {
        cell.publish(nanos);
    }
    UNSETTLED_OLDEST.publish(owed.oldest_seconds);
}

/// One relay pass's reading of what is still owed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Owed {
    /// Unsettled nanos per work type, in [`WorkType::ALL`] order.
    pub unsettled_nanos: [u64; 2],
    /// Age of the oldest unsettled line, in seconds.
    pub oldest_seconds: u64,
}
