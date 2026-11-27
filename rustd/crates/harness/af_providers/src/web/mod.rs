//! Platform-owned web search and page extraction over single-dispatch vendor APIs.

use std::time::Duration;

use af_crypto::secret::SecretString;
use af_wire::web::SearchResult;
use futures_util::future::BoxFuture;

use crate::Result;

mod client;
mod exa;
mod parallel;

pub use self::{exa::Exa, parallel::Parallel};

/// The result count requested from both web search APIs.
const RESULT_COUNT: usize = 10;

/// Safe facts about an upstream failure. Error pages and transport diagnostics
/// can echo private inputs and are deliberately excluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WebFailure {
    /// No complete HTTP response arrived before the dispatch budget expired.
    #[error("the web vendor connection failed")]
    Transport,
    /// Refusal by the vendor API itself, with its explicit retry interval.
    #[error("the web vendor answered HTTP {status}")]
    Status {
        /// HTTP response status of the vendor API.
        status: u16,
        /// Wait named in its response headers, when present.
        retry_after: Option<Duration>,
    },
    /// The upstream page's HTTP error inside a successful extract envelope.
    #[error("the requested page answered HTTP {status}")]
    Page {
        /// HTTP response status attributed to the single requested page.
        status: u16,
    },
    /// The answer could not be associated with the request or returned safely.
    #[error("the web vendor returned an invalid response")]
    InvalidResponse,
    /// The platform key cannot be sent in an HTTP header.
    #[error("the web vendor credential is unusable")]
    InvalidCredential,
}

/// A single physical query. Its caller authorizes and reserves each dispatch.
pub trait Vendor: Send + Sync {
    /// Search for up to ten web results within `timeout`, full reply included.
    ///
    /// # Errors
    /// Returns only a bounded failure class and the status and wait when known.
    fn search<'a>(
        &'a self,
        key: &'a SecretString,
        query: &'a str,
        timeout: Duration,
    ) -> BoxFuture<'a, Result<Vec<SearchResult>, WebFailure>>;
}
