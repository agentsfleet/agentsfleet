//! Explicit routing for identity-provider requests; ambient proxies stay ignored.

use std::fmt;
use std::time::Duration;

/// A deployment's deliberate choice of identity-provider network route.
#[derive(Clone, Default)]
pub enum Transport {
    /// Reach the provider without consulting proxy environment variables.
    #[default]
    Direct,
    /// Use the proxy the operator explicitly entrusted with identity traffic.
    TrustedProxy(Box<reqwest::Proxy>),
}

impl Transport {
    /// Starts with the direct client's deadline and disabled proxy discovery.
    pub(crate) fn builder(&self, timeout: Duration) -> reqwest::ClientBuilder {
        let builder = crate::direct(timeout);
        match self {
            Self::Direct => builder,
            Self::TrustedProxy(proxy) => builder.proxy((**proxy).clone()),
        }
    }
}

impl fmt::Debug for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // A proxy URL can contain credentials. Configuration diagnostics name
        // the chosen route without exposing its URL or authentication.
        f.write_str(match self {
            Self::Direct => "Direct",
            Self::TrustedProxy(_) => "TrustedProxy([REDACTED])",
        })
    }
}
