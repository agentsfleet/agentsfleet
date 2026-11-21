//! The default transport: one `reqwest` client, which follows no redirect.

use std::error::Error as StdError;
use std::fmt;

use bytes::Bytes;
use futures_util::{StreamExt as _, TryStreamExt as _};

use super::{Body, Send, SendError, Sent};

/// Whether a failure is the embedder's guard refusing its destination.
pub type IsRefused = fn(&(dyn StdError + 'static)) -> bool;

/// A [`Send`] over a `reqwest` client. Cloning it shares the client's
/// connection pool.
#[derive(Clone)]
pub struct Reqwest {
    client: reqwest::Client,
    refused: Option<IsRefused>,
}

impl fmt::Debug for Reqwest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reqwest").finish_non_exhaustive()
    }
}

impl Reqwest {
    /// A client that follows no redirect, so a key reaches only the host the
    /// route names.
    ///
    /// # Errors
    /// The client cannot be built.
    #[expect(
        clippy::disallowed_methods,
        reason = "guffbolt-ai links no agentsfleet crate, so its default client is built here and names its one bound: no redirect"
    )]
    pub fn new() -> reqwest::Result<Self> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self::with_client(client))
    }

    /// The embedder's own client. It must follow no redirect, as [`new`]'s
    /// does, or a key may reach a host the route never named.
    ///
    /// [`new`]: Self::new
    #[must_use]
    pub const fn with_client(client: reqwest::Client) -> Self {
        Self {
            client,
            refused: None,
        }
    }

    /// The same transport, answering [`SendError::Refused`] for a failure
    /// `refused` recognises as its client's guard refusing the destination.
    #[must_use]
    pub const fn refusing(mut self, refused: IsRefused) -> Self {
        self.refused = Some(refused);
        self
    }

    /// `failure` as the seam reads it.
    fn failed(&self, failure: reqwest::Error) -> SendError {
        if self.refused.is_some_and(|refused| refused(&failure)) {
            SendError::Refused(Box::new(failure))
        } else if failure.is_connect() || failure.is_timeout() {
            SendError::Unanswered(Box::new(failure))
        } else {
            SendError::Failed(Box::new(failure))
        }
    }
}

impl Send for Reqwest {
    fn send(&self, request: http::Request<Bytes>) -> Sent {
        let transport = self.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let response = transport
                .client
                .request(parts.method, parts.uri.to_string())
                .headers(parts.headers)
                .body(body)
                .send()
                .await
                .map_err(|failure| transport.failed(failure))?;
            let status = response.status();
            let headers = response.headers().clone();
            let body: Body = response
                .bytes_stream()
                .map_err(|failure| SendError::Failed(Box::new(failure)))
                .boxed();
            let mut response = http::Response::new(body);
            *response.status_mut() = status;
            *response.headers_mut() = headers;
            Ok(response)
        })
    }
}
