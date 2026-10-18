//! What the billing lifecycle records: reservations, settlements, write-offs,
//! unpriced tokens, and the sweeper's view of what is still owed.
//!
//! The counters are recorded by the one biller wrapper every money path holds,
//! so a hold or a settlement counts once whoever made it; the gauges are
//! published by the billing sweeper, which already reads the rows they
//! describe.

use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;

use crate::error::Result;
use crate::metrics::declared::billing as declared;
use crate::metrics::instrument::{Instruments, Reading};
use crate::metrics::label::billing::{Posture, ReserveOutcome, SettleOutcome, WorkType};
use crate::metrics::observed::Observed;
use crate::producers::installed;
use crate::semconv;

/// Unsettled nanos per work type, in [`WorkType::ALL`] order.
static UNSETTLED: [Observed; 2] = [Observed::new(), Observed::new()];

/// Age of the oldest unsettled line.
static UNSETTLED_OLDEST: Observed = Observed::new();

/// Nanos held and not yet captured.
static HELD: Observed = Observed::new();

/// The billing counters, claimed once.
#[derive(Debug)]
pub struct Handles {
    reserve: Counter<u64>,
    settle: Counter<u64>,
    written_off: Counter<u64>,
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
            UNSETTLED_OLDEST.load().map(Reading::unlabelled).into_iter().collect()
        })?;
        instruments.gauge_u64(&declared::BILLING_HELD_NANOS, || {
            HELD.load().map(Reading::unlabelled).into_iter().collect()
        })?;
        Ok(Self {
            reserve: instruments.counter_u64(&declared::BILLING_RESERVE_TOTAL)?,
            settle: instruments.counter_u64(&declared::BILLING_SETTLE_TOTAL)?,
            written_off: instruments.counter_u64(&declared::BILLING_WRITTEN_OFF_NANOS_TOTAL)?,
            unpriced: instruments.counter_u64(&declared::BILLING_UNPRICED_TOKENS_TOTAL)?,
            resettled: instruments.counter_u64(&declared::BILLING_SWEEPER_RESETTLED_TOTAL)?,
        })
    }
}

/// One reservation's answer.
pub fn reserved(work: WorkType, outcome: ReserveOutcome) {
    if let Some(producers) = installed() {
        producers.billing.reserve.add(
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

/// Nanos a settlement could not take from the balance.
pub fn written_off(work: WorkType, posture: Posture, nanos: u64) {
    if nanos == 0 {
        return;
    }
    if let Some(producers) = installed() {
        producers.billing.written_off.add(
            nanos,
            &[
                KeyValue::new(semconv::LABEL_WORK_TYPE, work.as_str()),
                KeyValue::new(semconv::LABEL_POSTURE, posture.as_str()),
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

/// Works the billing sweeper settled.
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

/// Publishes what the billing sweeper last read.
///
/// `None` withdraws every cell, for a pass whose read failed: a gap, never a
/// zero nobody measured.
pub fn owed_observed(owed: Option<Owed>) {
    let Some(owed) = owed else {
        for cell in &UNSETTLED {
            cell.withdraw();
        }
        UNSETTLED_OLDEST.withdraw();
        HELD.withdraw();
        return;
    };
    for (cell, nanos) in UNSETTLED.iter().zip(owed.unsettled_nanos) {
        cell.publish(nanos);
    }
    UNSETTLED_OLDEST.publish(owed.oldest_seconds);
    HELD.publish(owed.held_nanos);
}

/// One sweep's reading of what is still owed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Owed {
    /// Unsettled nanos per work type, in [`WorkType::ALL`] order.
    pub unsettled_nanos: [u64; 2],
    /// Age of the oldest unsettled line, in seconds.
    pub oldest_seconds: u64,
    /// Nanos held by open reservations and not yet captured.
    pub held_nanos: u64,
}
