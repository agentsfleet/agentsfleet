//! A verified HTTPS origin; the application route cannot be overridden.

use http::Uri;

use crate::{Result, error::ErrorKind};

/// The shared internal route on both native transports.
pub const PATH: &str = "/v1/runners/me/stream";

/// Parsed destination with no credentials, query or caller-selected route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    uri: Uri,
    host: String,
    port: u16,
    pub(crate) proxy: Option<crate::proxy::Proxy>,
    pub(crate) dial: Option<std::net::SocketAddr>,
}

impl Endpoint {
    /// Prove the origin before deriving the fixed stream URI.
    ///
    /// # Errors
    /// Refuses oversized input, malformed URLs and origins with credentials, paths, queries or fragments.
    pub fn parse(origin: &str) -> Result<Self> {
        let raw = garde::Unvalidated::new(Origin { text: origin }).validate()?;
        Self::from_valid(&raw)
    }

    fn from_valid(origin: &garde::Valid<Origin<'_>>) -> Result<Self> {
        let parsed = url::Url::parse(origin.text)?;
        if parsed.scheme() != "https"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != "/"
        {
            return Err(ErrorKind::Protocol.into());
        }
        let host = parsed
            .host_str()
            .ok_or(ErrorKind::Protocol)?
            .trim_matches(['[', ']'])
            .to_owned();
        let port = parsed.port_or_known_default().ok_or(ErrorKind::Protocol)?;
        let uri = parsed.join(PATH)?.as_str().parse()?;
        Ok(Self {
            uri,
            host,
            port,
            proxy: None,
            dial: None,
        })
    }

    /// Fixed URI, including the authority used by certificate verification.
    #[must_use]
    pub const fn uri(&self) -> &Uri {
        &self.uri
    }

    /// DNS name or literal address used as the verified peer identity.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Apply the host's explicit proxy policy once; HTTP/3 never bypasses it.
    ///
    /// # Errors
    /// Refuses unsupported proxy URLs. Standard `NO_PROXY` matching handles host, network and wildcard bypasses.
    pub fn with_environment_route(
        mut self,
        env: &(impl af_core::env::EnvSource + ?Sized),
    ) -> Result<Self> {
        self.proxy = crate::proxy::resolve(env, &self)?;
        Ok(self)
    }

    pub(crate) const fn port(&self) -> u16 {
        self.port
    }
}

#[derive(garde::Validate)]
struct Origin<'a> {
    #[garde(length(bytes, min = 1, max = 4096))]
    text: &'a str,
}
