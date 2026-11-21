//! What a chat run's earlier turns record: how many bytes each read, the
//! runs whose oldest turns the run frame cut, and the runs issued without
//! them.

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::HistoryCut;
use crate::producers::installed;
use crate::semconv;

/// Records the bytes of earlier turns one chat run read.
pub fn carried(bytes: usize) {
    if let Some(producers) = installed() {
        // Exact below 2^52 bytes, which a read that stops at one frame never nears.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a read stops past one 32 MiB frame, far inside f64's exact range"
        )]
        let bytes = bytes as f64;
        producers.fleet.history_bytes.record(bytes, &[]);
    }
}

/// Records one cut of a chat run's turns, under what made it.
pub fn cut(cause: HistoryCut) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .history_cuts
            .add(1, &[KeyValue::new(semconv::LABEL_REASON, cause.as_str())]);
    }
}

/// Records a chat run issued without its turns because the read failed.
pub fn read_failed() {
    if let Some(producers) = installed() {
        producers.fleet.history_read_failures.add(1, &[]);
    }
}
