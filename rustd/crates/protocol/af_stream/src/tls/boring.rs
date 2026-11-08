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
        let mut context = SslAcceptor::mozilla_modern(SslMethod::tls())?;
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

/// Trust roots; runners never load a client identity or CA signing key.
#[derive(Clone, Debug, Default)]
pub struct Trust {
    roots: Option<PathBuf>,
}

impl Trust {
    /// Add the supplied private root bundle to the OS trust store.
    #[must_use]
    pub fn with_roots(roots: impl Into<PathBuf>) -> Self {
        Self {
            roots: Some(roots.into()),
        }
    }

    fn configure(&self, context: &mut SslContextBuilder) -> Result<()> {
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
