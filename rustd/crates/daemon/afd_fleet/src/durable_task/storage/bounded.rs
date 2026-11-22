//! One span names each storage operation; the storage client's own deadlines bound it.

use crate::Result;
use tracing::Instrument as _;

pub(super) async fn operation<T>(
    name: &'static str,
    work: impl Future<Output = Result<T>>,
) -> Result<T> {
    work.instrument(tracing::info_span!("checkpoint_storage", operation = name))
        .await
}
