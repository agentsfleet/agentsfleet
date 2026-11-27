//! What the client's link runs over: any byte stream a [`ExecutorRoute`] dials.
//!
//! The JSON lines are the same over each. A sandbox that inherits a socket
//! pair is dialled once, with the stream it was handed; a VM's executor is
//! dialled again after every boot, so a link a park closed comes back.

use std::fmt;
use std::io;

use tokio::io::{AsyncRead, AsyncWrite};

/// A connected byte stream the link reads and writes its lines over.
pub trait ExecutorStream: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin> ExecutorStream for T {}

/// How a client reaches its executor, once at the start and again whenever
/// a call finds the link closed.
#[async_trait::async_trait]
pub trait ExecutorRoute: Send + Sync + fmt::Debug {
    /// A fresh stream to the executor.
    ///
    /// # Errors
    /// The executor cannot be reached, or this route dials no more.
    async fn dial(&self) -> io::Result<Box<dyn ExecutorStream>>;
}
