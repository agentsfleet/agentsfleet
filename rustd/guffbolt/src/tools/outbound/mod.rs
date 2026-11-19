//! Every request a tool sends off the machine goes through one hook.
//!
//! ```text
//!  http_request ─┐
//!  web_fetch ────┼─► Outbound::send ──► admit, rewrite or refuse ──► the network
//!  pushover ─────┤   Outbound::secret: a credential the tool places itself
//!  MCP over HTTP ┘   Outbound::connect: a server a client connects to
//! ```
//!
//! A tool drafts its request and words the answer; whether the request may
//! leave, what it carries and how its answer is masked are the hook's. An
//! embedder plugs its own egress guard in here, so a tool's request obeys
//! one network policy whichever tool drafted it. [`LocalOutbound`] is the
//! standalone default: HTTPS only, to the hosts it allows.

mod local;

use std::fmt;
use std::pin::Pin;

use crate::tool::{ErrorCode, ToolOutput};

pub use self::local::{Allowlist, LocalOutbound};

/// The most of a body a hook reads back; a longer one is cut and marked
/// truncated.
pub const RESPONSE_MAX_BYTES: usize = 1024 * 1024;

/// One outbound call's work.
pub type OutboundFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, Refused>> + Send + 'a>>;

/// Where a request may carry a credential placeholder the hook puts in
/// place as it sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// In the `Authorization` header only.
    Authorization,
    /// Nowhere: the request carries no placeholder.
    Nowhere,
}

/// Whether a request's redirects are followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redirects {
    /// Up to the hook's limit, each target admitted again.
    Follow,
    /// Never: a redirect is the answer.
    Stop,
}

/// A request a tool drafted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The method, as the model wrote it.
    pub method: String,
    /// The URL, as the model wrote it.
    pub url: String,
    /// The headers, in order.
    pub headers: Vec<(String, String)>,
    /// The body, sent as written.
    pub body: Option<String>,
    /// Where a credential placeholder may stand.
    pub placement: Placement,
    /// Whether its redirects are followed.
    pub redirects: Redirects,
}

/// What came back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Response {
    /// The status.
    pub status: u16,
    /// Where a redirect that was not followed points.
    pub location: Option<String>,
    /// The media type the answer names.
    pub content_type: Option<String>,
    /// The body as text, masked.
    pub body: String,
    /// Whether the body was cut at the hook's limit.
    pub truncated: bool,
    /// Whether bytes that were not UTF-8 were replaced.
    pub lossy: bool,
    /// How many redirects were followed.
    pub followed_redirects: u8,
}

/// Why a request did not leave, or got no answer: the code and the sentence
/// the model reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The code.
    pub code: ErrorCode,
    /// The sentence after it.
    pub detail: String,
}

impl Refused {
    /// A refusal under `code`, reading `detail`.
    #[must_use]
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl From<Refused> for ToolOutput {
    fn from(refused: Refused) -> Self {
        Self::failed(refused.code, &refused.detail)
    }
}

/// A server a client may connect to, and what its requests carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admitted {
    /// The URL to connect to, as the hook rewrote it.
    pub url: String,
    /// The headers every request to it carries, such as its credential.
    pub headers: Vec<(String, String)>,
}

/// The one place the generic tools build an HTTP client: bounded by
/// `timeout`, following no redirect itself, so every hop is the caller's to
/// admit.
#[expect(
    clippy::disallowed_methods,
    reason = "guffbolt links no agentsfleet crate, so the harness's one client builder lives here and names its bounds"
)]
pub fn client(timeout: std::time::Duration) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
}

/// The hook every network request goes through.
///
/// `tool` names the tool asking, for the hook's records. Each method
/// answers a boxed future so a tool holds any hook as `&dyn Outbound`.
pub trait Outbound: Send + Sync + fmt::Debug {
    /// Admits `request`, sends it and answers what came back, masked; a
    /// refusal is the code and sentence the model reads.
    fn send<'a>(&'a self, tool: &'a str, request: Request) -> OutboundFuture<'a, Response>;

    /// `text` with every secret the hook placed masked; `text` itself when
    /// none is in it.
    fn mask(&self, text: String) -> Pin<Box<dyn Future<Output = String> + Send + '_>>;

    /// The `field` of the secret `name`, for a tool that places a credential
    /// itself, such as in a body.
    fn secret<'a>(
        &'a self,
        tool: &'a str,
        name: &'a str,
        field: &'a str,
    ) -> OutboundFuture<'a, String>;

    /// Admits a connection to `url`, for a client that speaks to a server
    /// itself, such as an MCP client; the answer says where and with what.
    fn connect<'a>(&'a self, tool: &'a str, url: &'a str) -> OutboundFuture<'a, Admitted>;
}
