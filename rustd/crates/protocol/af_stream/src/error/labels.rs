//! Stable failure labels and human-readable operation names.

/// The operation whose absolute deadline expired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Acquiring frame memory before allocation.
    Memory,
    /// Reading a complete application frame.
    Read,
    /// Writing a complete application frame.
    Write,
    /// Resolving and connecting the endpoint.
    Connect,
    /// Verifying the peer and opening the transport.
    Handshake,
    /// Waiting for the HTTP response headers.
    Open,
    /// Closing native workers.
    Close,
}

impl Stage {
    /// Finite spelling for metrics and trace fields.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Read => "read",
            Self::Write => "write",
            Self::Connect => "connect",
            Self::Handshake => "handshake",
            Self::Open => "open",
            Self::Close => "close",
        }
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Memory => "reserving message memory",
            Self::Read => "reading a message",
            Self::Write => "writing a message",
            Self::Connect => "connecting to the server",
            Self::Handshake => "verifying the secure connection",
            Self::Open => "opening a stream",
            Self::Close => "stopping connection workers",
        })
    }
}

/// Stable categories; foreign error text never determines recovery policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The native network operation failed.
    Network,
    /// An absolute deadline expired.
    Timeout,
    /// The parent or caller cancelled the operation.
    Cancelled,
    /// Input violated the declared bounds or serialization format.
    Protocol,
    /// The channel or stream no longer has an owner.
    Closed,
    /// A supervised worker failed.
    Worker,
    /// Peer identity or local TLS configuration was refused; never downgrade.
    Tls,
    /// The peer did not support the offered native transport version.
    Unsupported,
}

impl Reason {
    /// Finite stable spelling; no peer-provided value enters a metric label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::Protocol => "protocol",
            Self::Closed => "closed",
            Self::Worker => "worker",
            Self::Tls => "tls",
            Self::Unsupported => "unsupported",
        }
    }
}
