use std::path::{Path, PathBuf};

use ::boring::ssl::{
    SslAcceptor, SslConnector, SslContextBuilder, SslFiletype, SslMethod, SslVerifyMode,
};

use crate::Result;

/// Server leaf chain and private key, loaded only by the daemon.
#[derive(Clone, Debug)]
pub struct Identity {
    pub(crate) chain: PathBuf,
    pub(crate) key: PathBuf,
}

impl Identity {
    /// Locate PEM files; building the listener validates their contents and pairing.
    #[must_use]
    pub fn new(chain: impl Into<PathBuf>, key: impl Into<PathBuf>) -> Self {
        Self {
            chain: chain.into(),
            key: key.into(),
        }
    }

    pub(crate) fn h2(&self) -> Result<SslAcceptor> {
        // The older `mozilla_modern` v4 preset disables TLS 1.3 in this crate.
        // Start from v5, then require TLS 1.3 through our shared policy below.
        let mut context = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls())?;
        super::key_exchange::require_context(&mut context)?;
        context.set_certificate_chain_file(&self.chain)?;
        context.set_private_key_file(&self.key, SslFiletype::PEM)?;
        context.check_private_key()?;
        context.set_alpn_select_callback(|_ssl, offered| {
            ::boring::ssl::select_next_proto(b"\x02h2", offered)
                .ok_or(::boring::ssl::AlpnError::ALERT_FATAL)
        });
        Ok(context.build())
    }
}

/// Trust roots and the datagram size a runner accepts; runners never load a client identity or CA
/// signing key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trust {
    roots: Option<PathBuf>,
    /// The largest UDP payload this runner tells the daemon it accepts.
    receive: usize,
}

impl Default for Trust {
    fn default() -> Self {
        Self {
            roots: None,
            receive: crate::mtu::MAX_DATAGRAM_BYTES,
        }
    }
}

impl Trust {
    /// Add the supplied private root bundle to the OS trust store.
    #[must_use]
    pub fn with_roots(roots: impl Into<PathBuf>) -> Self {
        Self {
            roots: Some(roots.into()),
            ..Self::default()
        }
    }

    /// Advertise `bytes` as the largest datagram this runner accepts, to model a runner with no
    /// ceiling of its own: the daemon's limit is then the only one left.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub const fn with_receive_limit(mut self, bytes: usize) -> Self {
        self.receive = bytes;
        self
    }

    pub(crate) const fn receive_limit(&self) -> usize {
        self.receive
    }

    fn configure(&self, context: &mut SslContextBuilder) -> Result<()> {
        super::key_exchange::require_context(context)?;
        context.set_verify(SslVerifyMode::PEER);
        context.set_default_verify_paths()?;
        if let Some(roots) = &self.roots {
            context.set_ca_file(roots)?;
        }
        Ok(())
    }

    pub(crate) fn h2(&self) -> Result<SslConnector> {
        let mut context = SslConnector::builder(SslMethod::tls())?;
        self.configure(&mut context)?;
        context.set_alpn_protos(b"\x02h2")?;
        Ok(context.build())
    }

    pub(crate) fn h3(&self) -> Result<SslContextBuilder> {
        let mut context = SslContextBuilder::new(SslMethod::tls())?;
        self.configure(&mut context)?;
        Ok(context)
    }

    /// Public root bundle selected by the caller, when present.
    #[must_use]
    pub fn roots(&self) -> Option<&Path> {
        self.roots.as_deref()
    }
}
