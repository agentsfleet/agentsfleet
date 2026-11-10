//! Outbound claims, counted per row taken; a rising `scan` share means lost pings.
use crate::error::Result;
use crate::metrics::declared::outbound as declared;
use crate::metrics::instrument::Instruments;
use crate::metrics::label::outbound::ClaimVia;
use crate::producers::installed;
use crate::semconv;
use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;

/// The outbound worker's instruments.
#[derive(Debug)]
pub struct Handles {
    claims: Counter<u64>,
}

impl Handles {
    pub(super) fn claim(instruments: &Instruments) -> Result<Self> {
        Ok(Self {
            claims: instruments.counter_u64(&declared::CLAIMS)?,
        })
    }
}

/// Records `rows` obligation rows one claim took.
pub fn claims(via: ClaimVia, rows: u64) {
    if let Some(producers) = installed() {
        producers
            .outbound
            .claims
            .add(rows, &[KeyValue::new(semconv::LABEL_VIA, via.as_str())]);
    }
}
