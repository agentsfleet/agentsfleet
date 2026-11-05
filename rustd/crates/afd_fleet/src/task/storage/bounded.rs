//! A single deadline covers the whole SDK operation, including streamed bytes.

use crate::{
    Result,
    error::{Error, ErrorKind},
};
use std::time::Duration;
use tracing::Instrument as _;

pub(super) const REQUEST_LIMIT: Duration = Duration::from_secs(60);

pub(super) async fn operation<T>(
    name: &'static str,
    work: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(
        REQUEST_LIMIT,
        work.instrument(tracing::info_span!("checkpoint_storage", operation = name)),
    )
    .await
    .map_err(|source| Error::from(ErrorKind::TaskStorageDeadline { source }))?
}
