//! What a chat lease's earlier turns record: how many bytes each carried,
//! which caps cut them, and the leases issued without them.

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::HistoryCut;
use crate::producers::installed;
use crate::semconv;

/// Records the bytes of earlier turns one chat lease carries.
pub fn carried(bytes: usize) {
    if let Some(producers) = installed() {
        // Exact below 2^52 bytes, which a capped window never nears.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a window is capped at 64 KiB, far inside f64's exact range"
        )]
        let bytes = bytes as f64;
        producers.fleet.history_bytes.record(bytes, &[]);
    }
}

/// Records one cap that cut a chat lease's turns.
pub fn cut(cap: HistoryCut) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .history_cuts
            .add(1, &[KeyValue::new(semconv::LABEL_REASON, cap.as_str())]);
    }
}

/// Records a chat lease issued without its turns because the read failed.
pub fn read_failed() {
    if let Some(producers) = installed() {
        producers.fleet.history_read_failures.add(1, &[]);
    }
}
