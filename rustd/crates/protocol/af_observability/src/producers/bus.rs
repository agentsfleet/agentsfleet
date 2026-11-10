//! Bus gaps, counted per subscription told; a rising share means a flaky broker.
use crate::error::Result;
use crate::metrics::declared::bus as declared;
use crate::metrics::instrument::Instruments;
use crate::metrics::label::bus::GapCause;
use crate::producers::installed;
use crate::semconv;
use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;

/// The bus's instruments.
#[derive(Debug)]
pub struct Handles {
    gaps: Counter<u64>,
}

impl Handles {
    pub(super) fn claim(instruments: &Instruments) -> Result<Self> {
        Ok(Self {
            gaps: instruments.counter_u64(&declared::GAPS)?,
        })
    }
}

/// Records `subscriptions` subscriptions told of one gap.
pub fn gaps(cause: GapCause, subscriptions: u64) {
    if let Some(producers) = installed() {
        producers.bus.gaps.add(
            subscriptions,
            &[KeyValue::new(semconv::LABEL_CAUSE, cause.as_str())],
        );
    }
}
