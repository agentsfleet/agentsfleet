//! Fakes for the suites that drive egress without a socket or a daemon.
//!
//! [`CountingMint`] answers one token and counts how often it was asked, so a
//! suite proves a lease mints once. [`RecordingTransport`] records what would
//! have reached the wire, credentials in place, and answers what its closure
//! says, so a suite proves both what was sent and that nothing was.

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use afd_core::clock::{Clock, FixedClock, UnixMillis};
use afr_secrets::Secret;

use crate::error::Result;
use crate::mint::{Mint, MintRefused, Minted};
use crate::refusal::Refusal;
use crate::transport::{Inbound, Outbound, Transport};

/// What [`CountingMint::never`] refuses with.
const NEVER: &str = "this suite mints no credential";

/// A mint answering one token that lives `lifetime_millis` from `clock`'s
/// reading, or refusing every time.
#[derive(Debug)]
pub struct CountingMint {
    answer: Answer,
    clock: FixedClock,
    asked: AtomicUsize,
}

#[derive(Debug)]
enum Answer {
    /// The first mint answers `token` itself, each later one `token-N`, so a
    /// suite tells a re-minted token from the one it replaced.
    Token {
        token: Secret,
        lifetime_millis: i64,
    },
    Refused(String),
}

impl CountingMint {
    /// A mint answering `token`, valid for `lifetime_millis` from `clock`.
    #[must_use]
    pub fn answering(token: &str, lifetime_millis: i64, clock: FixedClock) -> Self {
        Self::with(
            Answer::Token {
                token: Secret::new(token.to_owned()),
                lifetime_millis,
            },
            clock,
        )
    }

    /// A mint that refuses with `detail`.
    #[must_use]
    pub fn refusing(detail: &str, clock: FixedClock) -> Self {
        Self::with(Answer::Refused(detail.to_owned()), clock)
    }

    /// A mint for a suite whose leases mint nothing: it refuses, and a suite
    /// that wants to prove no mint happened reads [`CountingMint::asked`].
    #[must_use]
    pub fn never() -> Self {
        Self::refusing(NEVER, FixedClock::at(UnixMillis::EPOCH))
    }

    const fn with(answer: Answer, clock: FixedClock) -> Self {
        Self {
            answer,
            clock,
            asked: AtomicUsize::new(0),
        }
    }

    /// How many mints were asked for.
    #[must_use]
    pub fn asked(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl Mint for CountingMint {
    async fn mint(&self, _integration: &str) -> Result<Minted, MintRefused> {
        let asked = self.asked.fetch_add(1, Ordering::SeqCst) + 1;
        match &self.answer {
            Answer::Token {
                token,
                lifetime_millis,
            } => {
                let value = if asked == 1 {
                    token.expose().to_owned()
                } else {
                    format!("{}-{asked}", token.expose())
                };
                Ok(Minted::new(
                    Secret::new(value),
                    self.clock.now().saturating_add_millis(*lifetime_millis),
                ))
            }
            Answer::Refused(detail) => Err(MintRefused::new(detail.clone())),
        }
    }
}

/// One request as it would have reached the wire, credentials in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    /// The method.
    pub method: String,
    /// The URL.
    pub url: String,
    /// Every header, values exposed.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Option<String>,
}

impl Sent {
    /// The value of header `name`, if sent.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(sent, _)| sent.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// What a recording transport answers each request with.
type Reply = Box<dyn Fn(&Outbound) -> Result<Inbound, Refusal> + Send + Sync>;

/// A transport that records each request and answers what `reply` says.
pub struct RecordingTransport {
    reply: Reply,
    sent: Sender<Sent>,
}

impl RecordingTransport {
    /// A transport answering through `reply`, and the receiver every request
    /// it was handed arrives on.
    #[must_use]
    pub fn answering(
        reply: impl Fn(&Outbound) -> Result<Inbound, Refusal> + Send + Sync + 'static,
    ) -> (Self, Receiver<Sent>) {
        let (sent, received) = mpsc::channel();
        (
            Self {
                reply: Box::new(reply),
                sent,
            },
            received,
        )
    }

    /// A transport answering every request `status` with `body`.
    #[must_use]
    pub fn replying(status: u16, body: &str) -> (Self, Receiver<Sent>) {
        let body = body.to_owned();
        Self::answering(move |_outbound| Ok(inbound(status, &body)))
    }
}

impl fmt::Debug for RecordingTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordingTransport").finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl Transport for RecordingTransport {
    async fn send(&self, outbound: Outbound) -> Result<Inbound, Refusal> {
        let headers = outbound
            .headers()
            .iter()
            .map(|(name, value)| {
                let exposed = String::from_utf8_lossy(value.as_bytes()).into_owned();
                (name.as_str().to_owned(), exposed)
            })
            .collect();
        let record = Sent {
            method: outbound.method().to_string(),
            url: outbound.url().to_string(),
            headers,
            body: outbound.body().map(str::to_owned),
        };
        // A suite that dropped its receiver asserts nothing about what was sent.
        let _unread = self.sent.send(record);
        (self.reply)(&outbound)
    }
}

/// A plain-text answer with `status` and `body`.
#[must_use]
pub fn inbound(status: u16, body: &str) -> Inbound {
    Inbound {
        status,
        location: None,
        content_type: None,
        body: body.to_owned(),
        truncated: false,
    }
}
