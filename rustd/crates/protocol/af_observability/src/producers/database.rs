//! Database waits are recorded at acquisition; occupancy is read from pool atomics.

use std::sync::Arc;
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry::metrics::Histogram;

use crate::error::Result;
use crate::metrics::declared::database as declared;
use crate::metrics::instrument::Instruments;
use crate::metrics::label::database::{AcquireOutcome, PoolRole};
use crate::producers::{GaugeSources, installed};

/// Instruments owned by the database's producer.
#[derive(Debug)]
pub struct Handles {
    acquire: Histogram<f64>,
}

impl Handles {
    /// Claims the histogram and the nonblocking pool snapshot callback.
    ///
    /// # Errors
    /// Refuses a census/instrument disagreement.
    pub(super) fn claim(instruments: &Instruments, sources: &GaugeSources) -> Result<Self> {
        let pools = Arc::clone(&sources.database_pools);
        instruments.gauge_u64(&declared::POOL_CONNECTIONS, move || pools())?;
        Ok(Self {
            acquire: instruments.histogram_f64(&declared::POOL_ACQUIRE_DURATION)?,
        })
    }
}

/// Records one acquisition request's terminal answer, including cancellation.
pub fn acquired(role: PoolRole, outcome: AcquireOutcome, elapsed: Duration) {
    if let Some(producers) = installed() {
        producers.database.acquire.record(
            elapsed.as_secs_f64(),
            &[
                KeyValue::new(crate::semconv::LABEL_ROLE, role.as_str()),
                KeyValue::new(crate::semconv::LABEL_OUTCOME, outcome.as_str()),
            ],
        );
    }
}
