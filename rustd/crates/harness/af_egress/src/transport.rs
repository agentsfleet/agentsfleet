//! What leaves, what comes back, and the seam it crosses.
//!
//! An [`Outbound`] is built only by the lease's `Egress`, after admission, so
//! a [`Transport`] is handed nothing the policy did not admit. Production
//! sends through [`Network`](crate::Network); the tool suites hand in a
//! recording fake, so they prove what reaches the wire without a socket.

use std::fmt;

use reqwest::header::HeaderMap;
use reqwest::{Method, Url};

use crate::error::Result;
use crate::redirect::Target;

/// A request admitted and ready to leave, its credentials in place.
///
/// Its `Debug` names the method and the host only: the `Authorization`
/// header holds a secret, and the body may.
pub struct Outbound {
    pub(crate) method: Method,
    pub(crate) url: Url,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Option<String>,
    pub(crate) discard_redirect_body: bool,
}

impl Outbound {
    /// Skip intermediate bodies for an admitted bodyless GET; no request changes.
    #[must_use]
    pub fn for_redirect_read(mut self) -> Self {
        self.discard_redirect_body = self.method == Method::GET && self.body.is_none();
        self
    }
    /// The method.
    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// The URL.
    #[must_use]
    pub const fn url(&self) -> &Url {
        &self.url
    }

    /// The headers; `Authorization` is marked sensitive.
    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The body.
    #[must_use]
    pub fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }

    /// The URL's host, for a refusal or a log line.
    #[must_use]
    pub fn host(&self) -> &str {
        self.url.host_str().unwrap_or_default()
    }
}

impl fmt::Debug for Outbound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Outbound")
            .field("method", &self.method)
            .field("host", &self.host())
            .finish_non_exhaustive()
    }
}

/// What came back.
#[derive(Clone, PartialEq, Eq)]
pub struct Inbound {
    /// The status code.
    pub status: u16,
    /// Where a redirect points, as its origin alone: redirects are not
    /// followed, and a target's path and query may carry a token.
    pub location: Option<String>,
    /// The response's media type, as the upstream named it.
    pub content_type: Option<String>,
    /// The body, cut at the response cap.
    pub body: String,
    /// Whether the body was cut.
    pub truncated: bool,
    /// Whether invalid UTF-8 bytes were replaced while decoding the body.
    pub lossy: bool,
    /// Private continuation metadata, never rendered to the model.
    pub redirect: Option<Target>,
    /// Continuations completed within this HTTP tool call.
    pub followed_redirects: u8,
}

impl fmt::Debug for Inbound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inbound")
            .field("status", &self.status)
            .field("body_bytes", &self.body.len())
            .field("truncated", &self.truncated)
            .field("lossy", &self.lossy)
            .field("followed_redirects", &self.followed_redirects)
            .finish_non_exhaustive()
    }
}

/// Where an admitted request is sent.
#[async_trait::async_trait]
pub trait Transport: Send + Sync + fmt::Debug {
    /// Sends `outbound` and reads what comes back, up to the response cap.
    ///
    /// # Errors
    /// The host resolved to an address the runner never reaches, or no
    /// answer came back.
    async fn send(&self, outbound: Outbound) -> Result<Inbound>;
}
