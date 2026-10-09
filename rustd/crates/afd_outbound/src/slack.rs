//! Putting a fleet's answer back in the Slack thread the question came from.
//!
//! `chat.postMessage`, threaded under the mention that started the run. Two
//! inputs, from two places: the channel, the reply thread and the answer come
//! off the queue — the destination is the one the producer recorded when the
//! question arrived — and the bot token comes from the workspace's sealed
//! grant.
//!
//! # Both wire shapes are `serde` types, not field lookups
//!
//! The destination (`afd_connector::slack::Thread`, the one type the mention
//! producer writes and this reads) and Slack's answer are each a struct with
//! `Deserialize` on it, so the shape is stated once and the
//! "present but empty", "present but not a string" and "absent" cases are the
//! deserializer's problem rather than three hand-written guards that have to
//! agree.
//!
//! # Everything is a verdict, nothing is an error
//!
//! Reading either input can fail, and so can the POST. None of it returns
//! `Err`: the worker's only useful question is whether to try again, and
//! [`Verdict`] answers exactly that. An unreadable address and a revoked token
//! are `Permanent` for the same reason — the answer has nowhere to go and no
//! retry changes that. A vault that would not answer is `Retryable`, because
//! it is a blip rather than a fact about the job. The address is read first,
//! from the job alone, so a job that names nowhere costs no read and no
//! request.
//!
//! # No pool connection rides the vendor call
//!
//! A pool slot must never ride an HTTP call to somebody else's server. Slack
//! being slow would otherwise hold a Postgres connection for the length of its
//! outage. The poster holds no pool of its own: the token read borrows the
//! grant store's connection and returns it before [`SlackPoster::post`] is
//! entered — which the types enforce, since `post` never receives one.
//!
//! # A repeat looks before it posts
//!
//! `chat.postMessage` has no idempotency key, so every answer is posted
//! carrying its [`AnswerMarker`] as message metadata, and a repeat attempt
//! ([`Deliver::redeliver`]) first asks the thread whether that marker is
//! already there — see `afd_connector::slack::holds_answer` for what the
//! check can and cannot promise.

use afd_connector::slack::{AnswerMarker, Part, Stamp, Thread};
use afd_connector::{Grants, Provider};
use afd_core::id::Uuid7;
use afd_crypto::secret::SecretString;
use afd_dragonfly::OutboundDelivery;
use serde::{Deserialize, Serialize};

use crate::poster::{Deliver, Verdict};

mod verdict;

use self::verdict::{classify, destination, failed, verdict_of};

/// The method one answer is posted through.
const METHOD_POST_MESSAGE: &str = "/chat.postMessage";

/// How long one post may take before it is abandoned as retryable.
///
/// Invariant 4 — the deadline is at the call site. It also bounds a shutdown:
/// the worker finishes the attempt in flight before it stops, so the
/// supervisor's join waits out a repeat in the worst case — the thread check's
/// `afd_connector::slack::ANSWER_CHECK_DEADLINE` and then this — and the pair
/// has to leave room inside `JOIN_TIMEOUT`.
const POST_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

/// What a JSON request body is sent as.
///
/// Spelled rather than reached through `RequestBuilder::json`, because this
/// workspace resolves `reqwest` WITHOUT its `json` feature — see the note at
/// `afd_connector::exchange`, which declined to turn one on for one call site.
const CONTENT_TYPE_JSON: &str = "application/json; charset=utf-8";

/// HTTP statuses that decide a verdict, named once each (RULE UFS).
const STATUS_OK: u16 = 200;
/// See [`STATUS_OK`].
const STATUS_TOO_MANY_REQUESTS: u16 = 429;
/// See [`STATUS_OK`].
const STATUS_SERVER_ERROR_FLOOR: u16 = 500;

/// Failure reasons reached from more than one site, named once each (RULE UFS).
///
/// Each covers several distinct causes on purpose: a caller does the same thing
/// with every one of them, and the verdict at the call site — not the reason —
/// is what separates a retry from a give-up.
const REASON_TOKEN_LOAD_FAILED: &str = "slack_post_token_load_failed";

/// Logged when a job's address names nowhere this poster can post.
const REASON_ADDRESS_UNREADABLE: &str = "slack_post_address_unreadable";

/// Logged when a repeat found its answer already in the thread and posted
/// nothing.
const EVENT_ALREADY_IN_THREAD: &str = "slack_post_already_in_thread";

/// Logged when a repeat could not read the thread and posted anyway.
const EVENT_THREAD_CHECK_FAILED: &str = "slack_post_thread_check_failed";

/// Why a repeat could not check: the grant recorded no bot user, so no marker
/// in the thread can be proven this daemon's own.
const REASON_BOT_USER_UNRECORDED: &str = "bot_user_unrecorded";

/// What Slack answers a `chat.postMessage` with.
///
/// `ok` alone, because it is the only field that changes what happens next.
/// `#[serde(default)]` so a 200 that is JSON but not a Slack answer — a
/// proxy's error page, a captive portal — reads as NOT accepted rather than
/// failing to parse into something a caller might treat as success.
#[derive(Debug, Default, Deserialize)]
struct Accepted {
    #[serde(default)]
    ok: bool,
}

/// The body one answer is posted as.
///
/// A struct rather than `serde_json::json!` so the keys Slack expects are a
/// type, and `answer` — arbitrary model output — is escaped by `serde` on its
/// way out rather than interpolated.
#[derive(Debug, Serialize)]
struct Message<'a> {
    channel: &'a str,
    thread_ts: &'a str,
    text: &'a str,
    /// Which answer this is, so a repeat can find it in the thread.
    metadata: Stamp<'a>,
}

/// Posts a fleet's answer to Slack.
#[derive(Debug, Clone)]
pub struct SlackPoster {
    grants: Grants,
    http: reqwest::Client,
    api_base: String,
}

impl SlackPoster {
    /// Binds the poster to the grant store and an HTTP client.
    ///
    /// `api_base` is [`afd_connector::slack::SLACK_API_BASE`] in a
    /// deployment and a loopback in a test, so a test reaches a real socket
    /// without reaching Slack. The client is shared with the rest of the
    /// workspace rather than built here, so a connector adds no second HTTP
    /// stack.
    #[must_use]
    pub const fn new(grants: Grants, http: reqwest::Client, api_base: String) -> Self {
        Self {
            grants,
            http,
            api_base,
        }
    }

    /// Both inputs, the address first and from the job alone, and the marker
    /// `part` names: `None` for the answer, a line's place for an interim post.
    ///
    /// Answers a verdict directly on failure — see the module note on why
    /// nothing here is an error.
    async fn inputs(&self, job: &OutboundDelivery, part: Option<&Part>) -> Result<Inputs, Verdict> {
        let destination = destination(job)?;
        let Ok(workspace) = Uuid7::parse(&job.workspace_id) else {
            // An identifier this daemon queued that will not parse is this
            // build's own bug, not a transient: retrying re-runs the parse.
            return Err(failed(job, "identifier_unparseable", Verdict::Permanent));
        };

        let identity = match self.grants.bot_identity(&workspace, Provider::Slack).await {
            Ok(Some(identity)) => identity,
            // No handle, or no token in it: disconnected, uninstalled, malformed.
            // Retryable, so a reconnect inside the lanes' cycle budget delivers.
            Ok(None) => {
                return Err(failed(job, REASON_TOKEN_LOAD_FAILED, Verdict::Retryable));
            }
            Err(_unreadable) => {
                return Err(failed(job, REASON_TOKEN_LOAD_FAILED, Verdict::Retryable));
            }
        };

        let marker = AnswerMarker {
            fleet_id: job.fleet_id.clone(),
            event_id: job.event_id.clone(),
            part: part.cloned(),
        };
        Ok(Inputs {
            destination,
            token: identity.token,
            author: identity.user_id,
            marker,
        })
    }

    /// The POST itself, with no pool connection held — see the module note.
    async fn post(&self, job: &OutboundDelivery, inputs: &Inputs) -> Verdict {
        let body = serde_json::to_vec(&Message {
            channel: &inputs.destination.channel_id,
            thread_ts: &inputs.destination.thread_ts,
            text: &job.answer,
            metadata: inputs.marker.metadata(),
        });
        let Ok(body) = body else {
            return failed(job, "slack_post_body_unserializable", Verdict::Permanent);
        };

        let response = self
            .http
            .post(format!("{}{METHOD_POST_MESSAGE}", self.api_base))
            .bearer_auth(inputs.token.expose())
            .header(http::header::CONTENT_TYPE, CONTENT_TYPE_JSON)
            .timeout(POST_DEADLINE)
            .body(body)
            .send()
            .await;

        let Ok(response) = response else {
            // Transport, DNS, a fired deadline. All the same answer: Slack was
            // not reached, so nothing was said and saying it again may work.
            return failed(job, "slack_post_transport_failed", Verdict::Retryable);
        };
        let status = response.status().as_u16();
        let payload = response.text().await.unwrap_or_default();
        classify(status, &payload).unwrap_or_else(|reason| failed(job, reason, verdict_of(status)))
    }
}

impl SlackPoster {
    /// Posts `job` under the marker `part` names, once.
    pub(crate) async fn deliver_part(
        &self,
        job: &OutboundDelivery,
        part: Option<&Part>,
    ) -> Verdict {
        match self.inputs(job, part).await {
            Ok(inputs) => self.post(job, &inputs).await,
            Err(verdict) => verdict,
        }
    }

    /// Posts `job` under the marker `part` names, unless the thread already
    /// holds a message carrying that marker.
    pub(crate) async fn redeliver_part(
        &self,
        job: &OutboundDelivery,
        part: Option<&Part>,
    ) -> Verdict {
        let inputs = match self.inputs(job, part).await {
            Ok(inputs) => inputs,
            Err(verdict) => return verdict,
        };
        let held = match inputs.author.as_deref() {
            Some(author) => afd_connector::slack::holds_answer(
                &self.http,
                &self.api_base,
                &inputs.token,
                &inputs.destination,
                &inputs.marker,
                author,
            )
            .await
            .map_err(afd_connector::slack::Unavailable::as_str),
            None => Err(REASON_BOT_USER_UNRECORDED),
        };
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let workspace_id = job.workspace_id.as_str();
        let fleet_id = job.fleet_id.as_str();
        match held {
            Ok(true) => {
                tracing::info!(workspace_id, fleet_id, event = EVENT_ALREADY_IN_THREAD);
                Verdict::Delivered
            }
            Ok(false) => self.post(job, &inputs).await,
            Err(reason) => {
                // The code `failed` stamps on every other vendor failure, so
                // this line joins them for the same workspace.
                let error_code = afd_core::error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
                tracing::warn!(
                    error_code,
                    workspace_id,
                    fleet_id,
                    reason,
                    event = EVENT_THREAD_CHECK_FAILED
                );
                self.post(job, &inputs).await
            }
        }
    }
}

/// The answer: posted under the marker with no part.
impl Deliver for SlackPoster {
    fn deliver(&self, job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        self.deliver_part(job, None)
    }

    fn redeliver(&self, job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        self.redeliver_part(job, None)
    }
}

/// Everything one post needs, gathered before any vendor call begins.
#[derive(Debug)]
struct Inputs {
    destination: Thread,
    /// Still wrapped, so it zeroes on drop — see `Grants::bot_token`.
    token: SecretString,
    /// The bot user the grant recorded: a marker counts only from it.
    author: Option<String>,
    /// Which answer this is, as the post carries it and a repeat looks for it.
    marker: AnswerMarker,
}

#[cfg(test)]
mod tests;
