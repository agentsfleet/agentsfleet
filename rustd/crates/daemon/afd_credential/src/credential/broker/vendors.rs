//! The exchange behind the broker: the seam a mint goes through, and the
//! production dispatch over this deployment's platform credentials.
//!
//! Split from the broker by concern: these perform one exchange and know
//! nothing of the cache or of which connector set resolved it.

use std::time::Duration;

use serde_json::Value;
use zeroize::Zeroizing;

use super::Ask;
use crate::credential::outcome::{Minted, NEVER_EXPIRES, Outcome};
use crate::credential::platform::Platform;
use crate::credential::{github, oauth};
use crate::secrets::connector::{Connector, Exchange};

/// The vault-handle field a `static` connector's credential sits in.
const FIELD_TOKEN: &str = "token";

/// Performs one exchange, whatever the connector's kind.
///
/// The seam the production dispatch and a test's counting stand-in both sit
/// behind. Boxed rather than an `async fn` in the trait because the broker
/// holds it as `dyn` — one allocation per COLD mint, against an HTTPS round
/// trip.
pub trait Exchanger: std::fmt::Debug + Send + Sync {
    /// Mints through `connector`, or says why it could not.
    fn exchange<'a>(
        &'a self,
        connector: &'a dyn Connector,
        ask: Ask<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Outcome> + Send + 'a>>;
}

/// The production dispatch: this deployment's platform credentials, and one
/// HTTP client.
#[derive(Debug)]
pub struct Vendors {
    /// This deployment's own App and OAuth clients.
    platform: Platform,
    /// Shared, so the connection pool and the timeout are one decision.
    http: reqwest::Client,
    /// How long the tokens this dispatch mints are taken to live.
    lifetimes: Lifetimes,
}

/// How long a minted token is taken to live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lifetimes {
    /// An OAuth access token whose provider states no lifetime.
    pub assumed_access: Duration,
    /// A GitHub App installation token.
    pub installation_token: Duration,
}

impl Default for Lifetimes {
    fn default() -> Self {
        Self {
            assumed_access: oauth::DEFAULT_ACCESS_TTL,
            installation_token: github::INSTALLATION_TOKEN_TTL,
        }
    }
}

impl Vendors {
    /// The dispatch over `platform`, posting through `http`.
    #[must_use]
    pub fn new(platform: Platform, http: reqwest::Client) -> Self {
        Self {
            platform,
            http,
            lifetimes: Lifetimes::default(),
        }
    }

    /// This dispatch taking its minted tokens to live for `lifetimes`.
    #[must_use]
    pub const fn with_lifetimes(mut self, lifetimes: Lifetimes) -> Self {
        self.lifetimes = lifetimes;
        self
    }
}

impl Exchanger for Vendors {
    /// One `match`, and it is the only one in the broker.
    ///
    /// Adding a connector that mints the way an existing one does adds nothing
    /// here — that is what [`Exchange`] being data on the descriptor buys, and
    /// what the three refresh providers sharing one arm demonstrates.
    fn exchange<'a>(
        &'a self,
        connector: &'a dyn Connector,
        ask: Ask<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        Box::pin(async move {
            match connector.exchange() {
                Exchange::Stored => stored(ask.handle),
                Exchange::GithubApp => match self.platform.github() {
                    Some(app) => {
                        github::mint(github::Exchange {
                            app,
                            handle: ask.handle,
                            binding: ask.binding,
                            http: &self.http,
                            now: ask.now,
                            token_ttl: self.lifetimes.installation_token,
                        })
                        .await
                    }
                    // The tenant connected an App this deployment cannot
                    // authenticate as. An operator's to fix, and no retry
                    // helps — and it is not a failed exchange, because no
                    // exchange was attempted.
                    None => Outcome::Unconfigured,
                },
                Exchange::OAuthRefresh { token_url } => {
                    match self.platform.oauth(connector.name()) {
                        Some(app) => {
                            oauth::mint(oauth::Refresh {
                                app,
                                handle: ask.handle,
                                token_url,
                                http: &self.http,
                                now: ask.now,
                                assumed_ttl: self.lifetimes.assumed_access,
                            })
                            .await
                        }
                        None => Outcome::Unconfigured,
                    }
                }
            }
        })
    }
}

/// The `static` connector's whole exchange: there isn't one.
///
/// The stored handle already holds a usable credential, so this hands it back
/// with no upstream expiry. It reaches the broker even though the run path
/// delivers static credentials inline, because the runner may ask for anything
/// its fleet declared and refusing here would be a refusal the tenant cannot
/// act on.
pub(super) fn stored(handle: &Value) -> Outcome {
    handle
        .get(FIELD_TOKEN)
        .and_then(Value::as_str)
        .map_or(Outcome::ReconnectRequired, |token| {
            Outcome::Ok(Minted {
                token: Zeroizing::new(token.to_owned()),
                // A stored credential has no upstream expiry this daemon knows
                // of.
                expires_at: NEVER_EXPIRES,
                rotated_refresh_token: None,
            })
        })
}
