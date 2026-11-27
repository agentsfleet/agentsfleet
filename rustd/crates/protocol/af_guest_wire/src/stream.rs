//! What the link runs over: any connected byte stream, a Unix socket on a
//! host without a microVM and a vsock connection into one.

use tokio::io::{AsyncRead, AsyncWrite};

/// A connected byte stream either end reads and writes its lines over.
pub trait AgentStream: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin> AgentStream for T {}
