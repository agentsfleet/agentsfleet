//! What the client's link runs over: any byte stream a [`Transport`] dials.
//!
//! The JSON lines are the same over each. A sandbox that inherits a socket
//! pair is dialled once, with the stream it was handed; a VM's executor is
//! dialled again after every boot, so a link a park closed comes back.

use std::fmt;
use std::io;

use tokio::io::{AsyncRead, AsyncWrite};

/// A connected byte stream the link reads and writes its lines over.
pub trait Stream: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin> Stream for T {}

/// How a client reaches its executor, once at the start and again whenever
/// a call finds the link closed.
#[async_trait::async_trait]
pub trait Transport: Send + Sync + fmt::Debug {
    /// A fresh stream to the executor.
    ///
    /// # Errors
    /// The executor cannot be reached, or this transport dials no more.
    async fn dial(&self) -> io::Result<Box<dyn Stream>>;
}
