//! How a [`Client`] reaches its guest agent: a socket it waits on, a stream
//! already connected, or a route it dials and dials again.

use std::io;
use std::path::Path;
use std::time::Duration;

use backon::{ConstantBuilder, Retryable as _};
use tokio::net::UnixStream;

use super::Client;
use super::route::AgentRoute;
use af_guest_wire::Sizes;
use af_guest_wire::error::Result;
use af_guest_wire::timing::CALL_DEADLINE;

/// The pause between attempts to reach a socket that is not listening yet.
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(10);

impl Client {
    /// Connects to the guest agent on `socket`, retrying until it listens or
    /// `deadline` passes, holding the link to the `sizes` the guest agent was
    /// started with.
    ///
    /// The guest agent binds its socket as it starts, so a supervisor that
    /// starts one connects within the time a start takes rather than at once.
    ///
    /// # Errors
    /// The last attempt's failure once `deadline` passes, or at once for a
    /// failure that waiting cannot fix, such as a refused permission.
    pub async fn connect_within(socket: &Path, deadline: Duration, sizes: Sizes) -> Result<Self> {
        let attempts = deadline
            .as_millis()
            .div_euclid(CONNECT_RETRY_DELAY.as_millis());
        let policy = ConstantBuilder::new()
            .with_delay(CONNECT_RETRY_DELAY)
            .with_max_times(usize::try_from(attempts).unwrap_or(usize::MAX));
        let stream = (|| UnixStream::connect(socket))
            .retry(policy)
            .when(not_listening_yet)
            .await?;
        Ok(Self::over(stream, CALL_DEADLINE, sizes))
    }

    /// A client over `stream`, already connected to a guest agent: the engine's
    /// end of the socket pair its sandbox inherited, started with `sizes`.
    /// Nothing is retried, as nothing has to bind first, and a link that
    /// closes is not dialled again: its one stream was this one.
    ///
    /// # Errors
    /// The stream cannot be made non-blocking or join the runtime.
    pub fn connected(stream: std::os::unix::net::UnixStream, sizes: Sizes) -> Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self::over(
            UnixStream::from_std(stream)?,
            CALL_DEADLINE,
            sizes,
        ))
    }

    /// A client over whatever `route` dials, dialled once now and again
    /// whenever a call finds the link closed, holding the link to the `sizes`
    /// the guest agent was started with.
    ///
    /// # Errors
    /// The first dial fails.
    pub async fn dial(route: impl AgentRoute + 'static, sizes: Sizes) -> Result<Self> {
        let stream = route.dial().await?;
        Ok(Self::linked(
            stream,
            Some(Box::new(route)),
            CALL_DEADLINE,
            sizes,
        ))
    }
}

/// Whether a failed connect is a guest agent that has not bound its socket yet,
/// or has a socket file and is not accepting yet — the two that waiting fixes.
fn not_listening_yet(failure: &io::Error) -> bool {
    matches!(
        failure.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}
