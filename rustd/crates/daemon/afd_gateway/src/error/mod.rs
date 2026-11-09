//! Session failures preserve domain and transport causes behind the shared error shell.

use crate::Operation;

/// Fallible session operations retain the service that refused them.
pub type Result<T, E = Error> = core::result::Result<T, E>;

af_core::error_shell!(
    /// A safe session failure with its original cause and captured backtrace.
    pub struct Error(ErrorKind);
);

mod classify;
mod kind;

pub(crate) use kind::ErrorKind;

af_core::error_lifts!(Error, ErrorKind:
    std::num::TryFromIntError => Conversion,
    tokio::sync::oneshot::error::RecvError => Completion,
    http::header::ToStrError => Header,
    af_core::error::Error => Identifier,
    garde::Report => Bounds,
    serde_json::Error => Json,
    af_stream::Error => Transport,
    std::io::Error => Io,
    afd_db::Error => Database,
    afd_bus::Error => Bus,
    sqlx::Error => Query,
    afd_runner::Error => Runner,
    afd_fleet::Error => Fleet,
    afd_authoring::Error => Authoring,
    tokio::task::JoinError => Worker,
    http::Error => Http,
);

pub(crate) async fn within<T>(
    stop: &tokio_util::sync::CancellationToken,
    deadline: tokio::time::Instant,
    operation: Operation,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        biased;
        () = stop.cancelled() => Err(ErrorKind::Closed.into()),
        result = tokio::time::timeout_at(deadline, future) => {
            result.map_err(|source| ErrorKind::Timeout { operation, source })?
        }
    }
}

/// Drop undelivered private bytes while retaining the channel-closed error marker.
pub(crate) fn closed_send<T>(source: tokio::sync::mpsc::error::SendError<T>) -> Error {
    let tokio::sync::mpsc::error::SendError(undelivered) = source;
    drop(undelivered);
    ErrorKind::ChannelSend {
        source: tokio::sync::mpsc::error::SendError(()),
    }
    .into()
}
