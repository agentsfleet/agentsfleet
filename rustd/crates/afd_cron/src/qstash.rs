//! Telling the external scheduler what this daemon's schedules are.
//!
//! # Why this is a local adapter and not an SDK
//!
//! Both published client crates are unmaintained — 39 and 21 recent downloads
//! between them — and each pins an older `jsonwebtoken`, `reqwest` and `sha2`
//! than this workspace resolves. Adopting one would drag a second TLS backend
//! into a binary whose whole `rustls` invariant is that there is exactly one
//! `CryptoProvider`. What is actually needed is three calls and a typed reading
//! of the answer, and the typed reading is the deliverable.
//!
//! # The answer is classified, not just checked
//!
//! `is_success()` would collapse the two failures a caller acts on differently:
//! a vendor that could not be reached is retryable and `:sync` repairs it on
//! its own, while a vendor that answered 400 will answer 400 again forever and
//! retrying is an outbound load with no end. [`crate::Error`] keeps them apart
//! and the Zig's single `error.QStashRequestFailed` does not — the delta this
//! port closes rather than carries (RULE PORT).

use url::Url;

use crate::error::{self, Result};

/// Where the scheduler's management calls go when a deployment names no base.
///
/// A DEFAULT, not a constant the client reaches for directly. `qstash_client.zig`
/// took its base as a parameter and pinned that with a regression test — "outbound
/// url uses the configured api base, not a hardcoded host" — because a hardcoded
/// US host is a bug this product already shipped once and fixed in M105. A
/// deployment on another region names its own base; this value is what a
/// deployment that names none falls back to.
pub const API_BASE: &str = "https://qstash.upstash.io/v2";

/// The path a schedule is created under, before the destination.
const SCHEDULES_PATH: &str = "/schedules/";

/// The header carrying the cron expression a schedule fires on.
const HEADER_CRON: &str = "Upstash-Cron";

/// The header carrying the zone that expression is read in.
const HEADER_TIMEZONE: &str = "Upstash-Timezone";

/// The header this daemon's own bearer is presented in.
const HEADER_AUTHORIZATION: &str = "Authorization";

/// The scheme that bearer is presented under.
const BEARER_PREFIX: &str = "Bearer ";

/// The content type a fire body is posted as.
const HEADER_CONTENT_TYPE: &str = "Content-Type";

/// See [`HEADER_CONTENT_TYPE`].
const CONTENT_TYPE_JSON: &str = "application/json";

/// The path the signed fire arrives back on.
///
/// `cron/constants.zig`'s `ingress_path`, kept byte-for-byte: it is half of the
/// `sub` claim a fire token is verified against, so a divergence here would
/// make every schedule registered by one daemon unverifiable by the other.
pub const INGRESS_PATH: &str = "/v1/ingress/qstash/schedules";

/// The longest destination this daemon will register.
pub const MAX_DESTINATION_BYTES: usize = 2048;

/// Why a destination could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidDestination {
    /// The configured API URL is empty, or too long once the path is on it.
    Unusable,
    /// It carries a query or a fragment — see [`destination_url`].
    NotAPlainOrigin,
}

/// Where the scheduler should call back to, for this deployment.
///
/// # Errors
/// [`InvalidDestination`] rather than a silently truncated URL. The destination
/// rides RAW inside the provider's own request path, so a `?` or `#` here would
/// be read as the provider request's query or fragment and register a callback
/// this daemon never meant — `constants.zig` refuses at construction for
/// exactly this reason, and so does this.
pub fn destination_url(api_url: &str) -> Result<String, InvalidDestination> {
    let parsed = Url::parse(api_url).map_err(|_unparsed| InvalidDestination::Unusable)?;

    // Asked of the PARSED url rather than of the string. A `#` inside a
    // percent-encoded path is not a fragment and a `?` in userinfo is not a
    // query, so a substring search answers this question wrongly in both
    // directions — which for a value that rides raw inside the provider's own
    // request path is the difference between a callback that works and one
    // silently truncated at the first `?`.
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(InvalidDestination::NotAPlainOrigin);
    }

    let base = parsed.as_str().trim_end_matches('/');
    let destination = format!("{base}{INGRESS_PATH}");
    if destination.len() > MAX_DESTINATION_BYTES {
        return Err(InvalidDestination::Unusable);
    }
    Ok(destination)
}

/// What the scheduler answered a management call with.
#[derive(Debug, Clone)]
pub struct Registered {
    /// The key the scheduler now knows this schedule by.
    pub schedule_id: String,
}

/// The scheduler's management surface, over one HTTP client.
///
/// Cheap to clone: `reqwest::Client` is a handle over a shared connection pool,
/// so every clone shares one set of TLS sessions rather than opening its own.
#[derive(Debug, Clone)]
pub struct QStash {
    /// The client every call here goes out on.
    client: reqwest::Client,
    /// This deployment's bearer for the scheduler.
    token: String,
    /// Where a fire should arrive back.
    destination: String,
    /// Where this deployment's management calls go.
    api_base: String,
}

impl QStash {
    /// Binds a client to this deployment's credential, callback and scheduler.
    ///
    /// `api_base` carries no default here on purpose: a caller that forgets it
    /// should not silently get the US region. [`API_BASE`] is what the
    /// boot path resolves when a deployment names none, where the decision is
    /// visible.
    #[must_use]
    pub const fn new(
        client: reqwest::Client,
        token: String,
        destination: String,
        api_base: String,
    ) -> Self {
        Self {
            client,
            token,
            destination,
            api_base,
        }
    }

    /// Where this deployment's management calls go.
    #[must_use]
    pub fn api_base(&self) -> &str {
        &self.api_base
    }

    /// Where a fire from this deployment's schedules arrives.
    ///
    /// The value a fire token's `sub` claim is checked against — see
    /// [`crate::verifier::verify_at`].
    #[must_use]
    pub fn destination(&self) -> &str {
        &self.destination
    }

    /// This deployment's credential, as the scheme presents it.
    ///
    /// Composed once rather than at each call site: two spellings of a header
    /// value is two places for a missing space after `Bearer` to hide, and the
    /// failure it produces upstream is a 401 that reads like a wrong token.
    fn bearer(&self) -> String {
        format!("{BEARER_PREFIX}{}", self.token)
    }

    /// Registers or replaces a schedule upstream.
    ///
    /// # Errors
    /// Reports a scheduler that could not be reached, and one that answered and
    /// refused — see the module note on why those are two failures.
    pub async fn upsert(&self, cron: &str, timezone: &str, message: &str) -> Result<Registered> {
        let answer = self
            .client
            .post(format!(
                "{}{SCHEDULES_PATH}{}",
                self.api_base, self.destination
            ))
            .header(HEADER_AUTHORIZATION, self.bearer())
            .header(HEADER_CRON, cron)
            .header(HEADER_TIMEZONE, timezone)
            .header(HEADER_CONTENT_TYPE, CONTENT_TYPE_JSON)
            .body(message.to_owned())
            .send()
            .await?;

        let status = answer.status();
        if !status.is_success() {
            return Err(error::upstream_refused(status.as_u16()));
        }

        // The scheduler's own id for the schedule, which becomes `source_key`.
        // Read out of the answer rather than minted here: it is the value a
        // later delete has to name, and a value this daemon invented would
        // delete nothing.
        // Read as text and parsed here rather than through `Response::json`:
        // the workspace resolves `reqwest` without its `json` feature, and
        // turning one on for one call site would put `serde_json` inside every
        // other crate's HTTP client too.
        let body = answer.text().await?;
        let registered: ScheduleAnswer =
            serde_json::from_str(&body).map_err(|_unreadable| error::upstream_unreadable())?;
        Ok(Registered {
            schedule_id: registered.schedule_id,
        })
    }

    /// Removes a schedule upstream.
    ///
    /// A scheduler that says it has no such schedule is a SUCCESS here, not a
    /// failure: the caller's goal is that the schedule stop firing, and one
    /// that is already gone has met it. Treating a 404 as an error would leave
    /// a row stuck in `deleting` forever after a delete that half-succeeded.
    ///
    /// # Errors
    /// As [`Self::upsert`].
    pub async fn remove(&self, source_key: &str) -> Result<()> {
        let answer = self
            .client
            .delete(format!("{}{SCHEDULES_PATH}{source_key}", self.api_base))
            .header(HEADER_AUTHORIZATION, self.bearer())
            .send()
            .await?;

        let status = answer.status();
        if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        Err(error::upstream_refused(status.as_u16()))
    }
}

/// The scheduler's answer to a create.
#[derive(Debug, serde::Deserialize)]
struct ScheduleAnswer {
    /// Its own id for the schedule.
    #[serde(rename = "scheduleId")]
    schedule_id: String,
}
