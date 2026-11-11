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
//! retrying is an outbound load with no end. [`crate::Error`] keeps them apart.
//! Before either reaches the row, a failure that can heal is sent again a few
//! times on a short schedule; see [`retry`].

use af_core::id::Uuid7;
use afd_vault::Vault;

pub use self::destination::{INGRESS_PATH, MAX_DESTINATION_BYTES, destination_url};
pub use self::retry::Attempts;
use self::source::Source;

use crate::credentials::Credentials;
use crate::error::{self, Result};
use crate::upstream::{HEADER_SCHEDULE_ID, upstream_id};
use crate::verifier::VerifiedFire;

/// Where the scheduler's management calls go when a bag names no base.
///
/// A DEFAULT, not a constant the client reaches for directly: the base is the
/// bag's `url`, because a hardcoded US host breaks every deployment in another
/// region. Such a deployment names its own base; this value is what a bag that
/// names none falls back to.
pub const API_BASE: &str = "https://qstash.upstash.io/v2";

/// The path a schedule is created under, before the destination.
const SCHEDULES_PATH: &str = "/schedules/";

/// The header carrying the cron expression a schedule fires on.
const HEADER_CRON: &str = "Upstash-Cron";

/// The header carrying the zone that expression is read in.
const HEADER_TIMEZONE: &str = "Upstash-Timezone";

/// The content type a fire body is posted as.
const HEADER_CONTENT_TYPE: &str = "Content-Type";

/// See [`HEADER_CONTENT_TYPE`].
const CONTENT_TYPE_JSON: &str = "application/json";

/// What the scheduler answered a management call with.
#[derive(Debug, Clone)]
pub struct Registered {
    /// The id the scheduler filed this schedule under: the one pushed, since
    /// an answer naming any other is refused.
    pub schedule_id: String,
}

/// The scheduler's management surface, over one HTTP client.
///
/// Cheap to clone: `reqwest::Client` is a handle over a shared connection pool,
/// so every clone shares one set of TLS sessions rather than opening its own.
///
/// The credentials are NOT fields. They live in the admin workspace's
/// `qstash` bag and are read before each call, the way the invite mailer reads
/// `smtp-relay`, so a rotation reaches the next call without a restart.
#[derive(Debug, Clone)]
pub struct QStash {
    /// The client every call here goes out on.
    client: reqwest::Client,
    /// Where the credentials are read from.
    credentials: Source,
    /// Where a fire should arrive back.
    destination: String,
}

impl QStash {
    /// Binds a client to this deployment's vault, admin workspace and callback.
    ///
    /// `admin` is `None` on a deployment with no admin workspace, and every
    /// call then fails as a deployment with no credentials does.
    #[must_use]
    pub const fn new(
        client: reqwest::Client,
        vault: Vault,
        admin: Option<Uuid7>,
        destination: String,
    ) -> Self {
        Self {
            client,
            credentials: Source::Vault { vault, admin },
            destination,
        }
    }

    /// A client over fixed credentials, for a suite with no vault behind it.
    ///
    /// The signing keys are blank: no suite reads them through this client.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn fixed(
        client: reqwest::Client,
        token: af_crypto::secret::SecretString,
        destination: String,
        api_base: String,
    ) -> Self {
        let blank = || af_crypto::secret::SecretString::new(String::new());
        Self {
            client,
            credentials: Source::Fixed(Credentials {
                token,
                api_base,
                keys: crate::verifier::SigningKeys {
                    current: blank(),
                    next: blank(),
                },
            }),
            destination,
        }
    }

    /// The same fixed client, verifying fires against `keys`.
    ///
    /// For the fire-route suites, which mint tokens under known keys. A client
    /// over the vault ignores it: its keys are the bag's.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn with_keys(mut self, keys: crate::verifier::SigningKeys) -> Self {
        if let Source::Fixed(credentials) = &mut self.credentials {
            credentials.keys = keys;
        }
        self
    }

    /// The credentials as the `qstash` bag holds them now.
    ///
    /// # Errors
    /// An unusable bag (absent, malformed, a field missing) and a vault that
    /// would not answer, the second retryable.
    pub(crate) async fn credentials(&self) -> Result<Credentials> {
        self.credentials.read().await
    }

    /// Where a fire from this deployment's schedules arrives.
    ///
    /// The value a fire token's `sub` claim is checked against — see
    /// [`crate::verifier::verify_at`].
    #[must_use]
    pub fn destination(&self) -> &str {
        &self.destination
    }

    /// Registers or replaces `schedule` upstream, under the id the fire path
    /// reads back.
    ///
    /// The id is this daemon's, sent in [`HEADER_SCHEDULE_ID`]: the scheduler
    /// overwrites a schedule pushed again under the same id, so a repeat push
    /// replaces the registration rather than adding a second one beside it.
    /// See [`crate::upstream`] for why the fire path depends on it.
    ///
    /// # Errors
    /// Reports a scheduler that could not be reached, and one that answered and
    /// refused — see the module note on why those are two failures. An answer
    /// naming any id but the one sent is refused too; see [`confirmed`]. A
    /// failure that can heal is reported only after the last of `attempts`.
    pub async fn upsert(
        &self,
        schedule: &Uuid7,
        cron: &str,
        timezone: &str,
        message: &str,
        attempts: Attempts,
    ) -> Result<Registered> {
        let id = upstream_id(schedule);
        let credentials = self.credentials().await?;
        retry::send(attempts, &id, || {
            self.upsert_once(&credentials, &id, cron, timezone, message)
        })
        .await
    }

    /// One push of the schedule filed under `id`.
    async fn upsert_once(
        &self,
        credentials: &Credentials,
        id: &str,
        cron: &str,
        timezone: &str,
        message: &str,
    ) -> Result<Registered> {
        let answer = self
            .client
            .post(format!(
                "{}{SCHEDULES_PATH}{}",
                credentials.api_base, self.destination
            ))
            .timeout(retry::ATTEMPT_TIMEOUT)
            .bearer_auth(credentials.token.expose())
            .header(HEADER_SCHEDULE_ID, id)
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

        // Read as text and parsed here rather than through `Response::json`:
        // the workspace resolves `reqwest` without its `json` feature, and
        // turning one on for one call site would put `serde_json` inside every
        // other crate's HTTP client too.
        let body = answer.text().await?;
        confirmed(id.to_owned(), &body)
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
    pub async fn remove(&self, provider_schedule_id: &str, attempts: Attempts) -> Result<()> {
        let credentials = self.credentials().await?;
        retry::send(attempts, provider_schedule_id, || {
            self.remove_once(&credentials, provider_schedule_id)
        })
        .await
    }

    /// One removal of the schedule filed under `provider_schedule_id`.
    async fn remove_once(
        &self,
        credentials: &Credentials,
        provider_schedule_id: &str,
    ) -> Result<()> {
        let answer = self
            .client
            .delete(format!(
                "{}{SCHEDULES_PATH}{provider_schedule_id}",
                credentials.api_base
            ))
            .timeout(retry::ATTEMPT_TIMEOUT)
            .bearer_auth(credentials.token.expose())
            .send()
            .await?;

        let status = answer.status();
        if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        Err(error::upstream_refused(status.as_u16()))
    }
}

#[async_trait::async_trait]
impl crate::scheduler::Scheduler for QStash {
    async fn upsert(
        &self,
        schedule: &Uuid7,
        cron: &str,
        timezone: &str,
        message: &str,
        attempts: Attempts,
    ) -> Result<Registered> {
        Self::upsert(self, schedule, cron, timezone, message, attempts).await
    }

    async fn remove(&self, provider_schedule_id: &str, attempts: Attempts) -> Result<()> {
        Self::remove(self, provider_schedule_id, attempts).await
    }

    /// Checked against the keys the `qstash` bag holds now, and this
    /// deployment's destination as the token's subject.
    async fn verify(&self, signature: &str, body: &[u8]) -> Result<VerifiedFire> {
        crate::verifier::bounded(signature)?;
        let credentials = self.credentials().await?;
        crate::verifier::verify_at(&credentials.keys, &self.destination, signature, body)
    }
}

/// The scheduler's answer to a create.
#[derive(Debug, serde::Deserialize)]
struct ScheduleAnswer {
    /// The id it filed the schedule under.
    #[serde(rename = "scheduleId")]
    schedule_id: String,
}

/// The registration the scheduler's answer to a push of `sent` confirms.
///
/// Refused unless the answer names `sent`. A scheduler that never read the id
/// header — a proxy that stripped it, an API version that ignores it — files
/// the schedule under an id of its own, and adopting that id would mark the row
/// synced while every fire it sends is dropped as naming no schedule. Refused
/// as unreadable instead, so the row records `failed`. Its reason reads as the
/// scheduler being unavailable, the sentence any unreadable answer gets.
fn confirmed(sent: String, body: &str) -> Result<Registered> {
    let answer: ScheduleAnswer =
        serde_json::from_str(body).map_err(|_unreadable| error::upstream_unreadable())?;
    if answer.schedule_id != sent {
        return Err(error::upstream_unreadable());
    }
    Ok(Registered { schedule_id: sent })
}

mod destination;
mod retry;
mod source;

/// The retry schedule's floor, bound for the retry suite so the least wait it
/// asserts follows the schedule instead of a copied number.
#[cfg(feature = "test-util")]
pub mod retry_schedule {
    /// The first wait between attempts; the second doubles from it.
    pub const FIRST_DELAY: std::time::Duration = super::retry::FIRST_DELAY;
}

#[cfg(test)]
mod tests;
