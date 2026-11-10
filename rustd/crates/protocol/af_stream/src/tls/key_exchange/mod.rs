//! The runner channel requires hybrid key agreement under TLS 1.3.

use af_observability::metrics::label::fleet::{StreamKeyExchange, StreamPeer};
use boring::ssl::{SslContextBuilder, SslRef, SslVersion};

use crate::{Result, Transport, error::ErrorKind};

/// Finite public algorithm labels; these contain no key material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyExchange {
    /// Hybrid conventional and post-quantum key agreement.
    X25519MlKem768,
    /// Conventional X25519 key agreement.
    X25519,
    /// Conventional NIST P-256 key agreement.
    P256,
    /// Conventional NIST P-384 key agreement.
    P384,
    /// No recognized negotiated group is available.
    Other,
}

const RECOGNIZED: [(KeyExchange, &str); 4] = [
    (KeyExchange::X25519MlKem768, "X25519MLKEM768"),
    (KeyExchange::X25519, "X25519"),
    (KeyExchange::P256, "P-256"),
    (KeyExchange::P384, "P-384"),
];

impl KeyExchange {
    /// Stable spelling for connection observations.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X25519MlKem768 => "x25519_mlkem768",
            Self::X25519 => "x25519",
            Self::P256 => "p256",
            Self::P384 => "p384",
            Self::Other => "other",
        }
    }
}

impl From<&SslRef> for KeyExchange {
    fn from(ssl: &SslRef) -> Self {
        match RECOGNIZED
            .into_iter()
            .find(|(_, name)| Some(*name) == ssl.curve_name())
        {
            Some((group, _)) => group,
            None => Self::Other,
        }
    }
}

pub(crate) const REQUIRED_GROUP: &str = "X25519MLKEM768";

pub(crate) fn require_context(context: &mut SslContextBuilder) -> Result<()> {
    context.set_min_proto_version(Some(SslVersion::TLS1_3))?;
    context.set_curves_list(REQUIRED_GROUP)?;
    Ok(())
}

/// QUIC acceptors expose this object before consuming the first Initial packet.
/// Configure the actual session rather than a hook that can select a default context.
pub(crate) fn require_session(ssl: &mut SslRef) -> Result<()> {
    ssl.set_min_proto_version(Some(SslVersion::TLS1_3))?;
    ssl.set_curves_list(REQUIRED_GROUP)?;
    Ok(())
}

impl KeyExchange {
    pub(crate) fn require(self, transport: Transport, peer: StreamPeer) -> Result<Self> {
        crate::telemetry::security(transport, peer, self);
        self.check()
    }

    pub(crate) fn check(self) -> Result<Self> {
        if self != Self::X25519MlKem768 {
            return Err(ErrorKind::KeyExchangeRefused { negotiated: self }.into());
        }
        Ok(self)
    }
}

impl From<KeyExchange> for StreamKeyExchange {
    fn from(group: KeyExchange) -> Self {
        match group {
            KeyExchange::X25519MlKem768 => Self::X25519MlKem768,
            KeyExchange::X25519 => Self::X25519,
            KeyExchange::P256 => Self::P256,
            KeyExchange::P384 => Self::P384,
            KeyExchange::Other => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests;
