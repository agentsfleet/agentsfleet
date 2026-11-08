//! One TLS 1.3 preference and the key exchange actually negotiated by `BoringSSL`.

use boring::ssl::{SslContextBuilder, SslInfoCallbackMode, SslRef, SslVersion};
use tokio::sync::watch;

use crate::Result;

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

const PREFERENCE: [(KeyExchange, &str); 4] = [
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
        match PREFERENCE
            .into_iter()
            .find(|(_, name)| Some(*name) == ssl.curve_name())
        {
            Some((group, _)) => group,
            None => Self::Other,
        }
    }
}

pub(crate) fn prefer(context: &mut SslContextBuilder) -> Result<()> {
    context.set_min_proto_version(Some(SslVersion::TLS1_3))?;
    context.set_curves_list(&PREFERENCE.map(|(_, name)| name).join(":"))?;
    Ok(())
}

pub(crate) fn observe(context: &mut SslContextBuilder) -> watch::Receiver<Option<KeyExchange>> {
    let (sender, receiver) = watch::channel(None);
    context.set_info_callback(move |ssl, mode, _value| {
        if mode == SslInfoCallbackMode::HANDSHAKE_DONE {
            sender.send_replace(Some(KeyExchange::from(ssl)));
        }
    });
    receiver
}

#[cfg(test)]
mod tests;
