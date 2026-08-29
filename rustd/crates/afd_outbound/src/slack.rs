//! Putting a fleet's answer back in the Slack thread the question came from.
//!
//! `chat.postMessage`, threaded under the mention that started the run. Three
//! inputs, from three places: the channel and the reply thread come from the
//! originating event's `request_json`, the bot token comes from the workspace's
//! sealed grant, and the answer comes off the queue.
//!
//! # Both wire shapes are `serde` types, not field lookups
//!
//! `post.zig` walks `std.json.Value` with a `strField` helper because Zig has
//! no derive. Here the destination and Slack's answer are each a struct with
//! `Deserialize` on it, so the shape is stated once in the type and the
//! "present but empty", "present but not a string" and "absent" cases are the
//! deserializer's problem rather than three hand-written guards that have to
//! agree.
//!
//! # Everything is a verdict, nothing is an error
//!
//! Reading either input can fail, and so can the POST. None of it returns
//! `Err`: the worker's only useful question is whether to try again, and
//! [`Verdict`] answers exactly that. A missing event row and a revoked token
//! are `Permanent` for the same reason — the answer has nowhere to go and no
//! retry changes that. A pool that would not lend a connection is `Retryable`,
//! because it is a blip rather than a fact about the job. `post.zig` reaches
//! the same three answers.
//!
//! # The pool connection is released before the vendor is dialled
//!
//! A pool slot must never ride an HTTP call to somebody else's server. Slack
//! being slow would otherwise hold a Postgres connection for the length of its
//! outage, and a handful of stalled deliveries would starve the request path
//! of connections for a reason nothing in Postgres could explain. The reads
//! happen first, and the connection is dropped before [`SlackPoster::post`] is
//! entered — which the types enforce, since `post` never receives one.

use afd_connector::{Grants, Provider};
use afd_core::id::Uuid7;
use afd_crypto::secret::SecretBytes;
use afd_db::Db;
use afd_redis::OutboundDelivery;
use serde::{Deserialize, Serialize};

use crate::poster::{Deliver, Verdict};

/// The Slack Web API root this deployment posts to.
///
/// Overridable so a test can point the poster at a loopback fake — the same
/// seam `post.zig` carries for the same reason. Not a general knob: a
/// deployment has no reason to talk to a different Slack.
pub const SLACK_API_BASE: &str = "https://slack.com/api";

/// The method one answer is posted through.
const METHOD_POST_MESSAGE: &str = "/chat.postMessage";

/// How long one post may take before it is abandoned as retryable.
///
/// Invariant 4 — the deadline is at the call site. It also bounds a shutdown:
/// the worker finishes the attempt in flight before it stops, so this is what
/// the supervisor's join waits out in the worst case, and it has to leave room
/// inside `JOIN_TIMEOUT`.
const POST_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

/// What a JSON request body is sent as.
///
/// Spelled rather than reached through `RequestBuilder::json`, because this
/// workspace resolves `reqwest` WITHOUT its `json` feature — see the note at
/// `afd_connector::exchange`, which declined to turn one on for one call site.
const CONTENT_TYPE_JSON: &str = "application/json; charset=utf-8";

/// Where the channel and the reply thread are read from.
///
/// `request_json` is what the mention ingress wrote when the question arrived,
/// so the answer is threaded under the message that asked it rather than
/// dropped at the bottom of a channel.
const SELECT_EVENT_REQUEST: &str = "SELECT request_json::text FROM core.fleet_events \
                                    WHERE fleet_id = $1::uuid AND event_id = $2";

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
/// See [`REASON_TOKEN_LOAD_FAILED`].
const REASON_EVENT_LOAD_FAILED: &str = "slack_post_event_load_failed";

/// Where in a Slack thread an answer belongs.
///
/// A DATA FORMAT: these are the keys `events.zig`'s `buildRequestJson` writes,
/// so the field names are `serde`'s contract with the other daemon and are not
/// this crate's to rename.
///
/// Both fields are required, which is the whole guard: an event carrying
/// neither is not a Slack mention whatever queued it, and one carrying an empty
/// channel posts nowhere — [`non_empty`] is what turns `""` into the same
/// answer as absent, before a request is built rather than after it fails.
#[derive(Debug, Deserialize)]
struct Destination {
    #[serde(rename = "channel_id", deserialize_with = "non_empty")]
    channel: String,
    #[serde(rename = "reply_thread_ts", deserialize_with = "non_empty")]
    thread: String,
}

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
/// A struct rather than `serde_json::json!` so the three keys Slack expects are
/// a type, and `answer` — arbitrary model output — is escaped by `serde` on its
/// way out rather than interpolated.
#[derive(Debug, Serialize)]
struct Message<'a> {
    channel: &'a str,
    thread_ts: &'a str,
    text: &'a str,
}

/// Refuses a string field that is present and empty.
///
/// `""` and absent mean the same thing to every caller here, and the type
/// system only distinguishes them if something says so. Saying it once in a
/// deserializer beats saying it at each read.
fn non_empty<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom("empty"));
    }
    Ok(value)
}

/// Posts a fleet's answer to Slack.
#[derive(Debug, Clone)]
pub struct SlackPoster {
    database: Db,
    grants: Grants,
    http: reqwest::Client,
    api_base: String,
}

impl SlackPoster {
    /// Binds the poster to the pool, the grant store and an HTTP client.
    ///
    /// `api_base` is [`SLACK_API_BASE`] in a deployment and a loopback in a
    /// test. The client is shared with the rest of the workspace rather than
    /// built here, so a connector adds no second HTTP stack.
    #[must_use]
    pub const fn new(
        database: Db,
        grants: Grants,
        http: reqwest::Client,
        api_base: String,
    ) -> Self {
        Self {
            database,
            grants,
            http,
            api_base,
        }
    }

    /// Both stored inputs, with the pool connection released before returning.
    ///
    /// Answers a verdict directly on failure — see the module note on why
    /// nothing here is an error.
    async fn inputs(&self, job: &OutboundDelivery) -> Result<Inputs, Verdict> {
        let (Ok(fleet), Ok(workspace)) =
            (Uuid7::parse(&job.fleet_id), Uuid7::parse(&job.workspace_id))
        else {
            // An identifier this daemon queued that will not parse is this
            // build's own bug, not a transient: retrying re-runs the parse.
            return Err(failed(job, "identifier_unparseable", Verdict::Permanent));
        };

        let destination = self.destination(job, &fleet).await?;
        let token = match self.grants.bot_token(&workspace, Provider::Slack).await {
            Ok(Some(token)) => token,
            // No handle, or one carrying no token: uninstalled, disconnected,
            // or a grant that landed malformed. Reconnecting is the only fix.
            Ok(None) => {
                return Err(failed(job, REASON_TOKEN_LOAD_FAILED, Verdict::Permanent));
            }
            Err(_unreadable) => {
                return Err(failed(job, REASON_TOKEN_LOAD_FAILED, Verdict::Retryable));
            }
        };

        Ok(Inputs { destination, token })
    }

    /// Where the answer goes, read from the event that asked the question.
    async fn destination(
        &self,
        job: &OutboundDelivery,
        fleet: &Uuid7,
    ) -> Result<Destination, Verdict> {
        let Ok(mut connection) = self.database.acquire().await else {
            // A pool that will not lend is a blip, and the answer is still
            // deliverable in a moment.
            return Err(failed(job, "pool_unavailable", Verdict::Retryable));
        };
        let row: Option<(String,)> = sqlx::query_as(SELECT_EVENT_REQUEST)
            .bind(fleet.as_str())
            .bind(&job.event_id)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(|_unreadable| failed(job, REASON_EVENT_LOAD_FAILED, Verdict::Retryable))?;
        drop(connection);

        // Gone, unreadable, or naming nowhere to post — one answer for all
        // three, because a caller does the same thing with each: the event is
        // not one this poster can thread an answer under, and no retry makes
        // it one.
        row.and_then(|(request,)| serde_json::from_str::<Destination>(&request).ok())
            .ok_or_else(|| failed(job, REASON_EVENT_LOAD_FAILED, Verdict::Permanent))
    }

    /// The POST itself, with no pool connection held — see the module note.
    async fn post(&self, job: &OutboundDelivery, inputs: &Inputs) -> Verdict {
        let Ok(token) = std::str::from_utf8(inputs.token.expose()) else {
            return failed(job, REASON_TOKEN_LOAD_FAILED, Verdict::Permanent);
        };
        let body = serde_json::to_vec(&Message {
            channel: &inputs.destination.channel,
            thread_ts: &inputs.destination.thread,
            text: &job.answer,
        });
        let Ok(body) = body else {
            return failed(job, "slack_post_body_unserializable", Verdict::Permanent);
        };

        let response = self
            .http
            .post(format!("{}{METHOD_POST_MESSAGE}", self.api_base))
            .bearer_auth(token)
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

impl Deliver for SlackPoster {
    async fn deliver(&self, job: &OutboundDelivery) -> Verdict {
        match self.inputs(job).await {
            Ok(inputs) => self.post(job, &inputs).await,
            Err(verdict) => verdict,
        }
    }
}

/// Everything one post needs, gathered before any vendor call begins.
#[derive(Debug)]
struct Inputs {
    destination: Destination,
    /// Still wrapped, so it zeroes on drop — see `Grants::bot_token`.
    token: SecretBytes,
}

/// The verdict a status and a body earn, or the event a failure is logged as.
///
/// `Ok` for the one success. `Err` carries the event name, which the caller
/// pairs with [`verdict_of`]. Split because the two are different facts: the
/// verdict decides what happens next, and the event is what an operator greps
/// — and §8A asks a port to keep the Zig's event spellings, which a verdict
/// enum has no room to carry.
fn classify(status: u16, payload: &str) -> Result<Verdict, &'static str> {
    if status == STATUS_TOO_MANY_REQUESTS || status >= STATUS_SERVER_ERROR_FLOOR {
        return Err("slack_post_retryable");
    }
    if status != STATUS_OK {
        return Err("slack_post_unexpected_status");
    }
    // Slack answers 200 with `{"ok": false}` for app-level refusals — a channel
    // that is gone, a scope that was never granted. The status alone would read
    // every one of those as a delivered answer.
    if serde_json::from_str::<Accepted>(payload).is_ok_and(|body| body.ok) {
        Ok(Verdict::Delivered)
    } else {
        Err("slack_post_app_error")
    }
}

/// The verdict a status earns once [`classify`] has refused it.
const fn verdict_of(status: u16) -> Verdict {
    if status == STATUS_TOO_MANY_REQUESTS || status >= STATUS_SERVER_ERROR_FLOOR {
        Verdict::Retryable
    } else {
        // Includes the 200 that carried `{"ok": false}`: a bad scope or a
        // deleted channel refuses identically on every retry.
        Verdict::Permanent
    }
}

/// Logs why a delivery did not land and returns the verdict it earns.
///
/// The event goes to the operator and never to Slack. One site, so a failure
/// added later cannot be the one that forgets to say anything.
fn failed(job: &OutboundDelivery, event: &'static str, verdict: Verdict) -> Verdict {
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let error_code = afd_core::error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
    let workspace_id = job.workspace_id.as_str();
    let fleet_id = job.fleet_id.as_str();
    let reason = match verdict {
        Verdict::Delivered | Verdict::Permanent => "permanent",
        Verdict::Retryable => "retryable",
    };
    tracing::warn!(error_code, workspace_id, fleet_id, reason, event);
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The verdict a real response earns, through both halves of the split.
    fn verdict(status: u16, payload: &str) -> Verdict {
        classify(status, payload).unwrap_or_else(|_event| verdict_of(status))
    }

    /// The destination a stored `request_json` resolves to, if any.
    fn destination(stored: &str) -> Option<Destination> {
        serde_json::from_str(stored).ok()
    }

    #[test]
    fn test_only_a_200_that_slack_accepted_is_delivered() {
        assert_eq!(
            verdict(200, r#"{"ok":true,"ts":"1700000000.000100"}"#),
            Verdict::Delivered
        );
    }

    /// The case a status check alone gets wrong: Slack refuses at the
    /// application layer with a 200.
    #[test]
    fn test_a_200_carrying_ok_false_is_permanent() {
        assert_eq!(
            verdict(200, r#"{"ok":false,"error":"channel_not_found"}"#),
            Verdict::Permanent,
            "a deleted channel refuses identically on every retry"
        );
    }

    /// A 200 that is not a Slack answer at all — a proxy's error page, a
    /// captive portal. Not acceptance.
    #[test]
    fn test_a_200_that_is_not_slack_json_is_not_delivered() {
        for payload in ["", "not json", "[]", r#"{"ok":"true"}"#, "{}"] {
            assert_ne!(
                verdict(200, payload),
                Verdict::Delivered,
                "`{payload}` is not Slack saying it accepted the message"
            );
        }
    }

    #[test]
    fn test_a_rate_limit_and_a_server_error_are_retryable() {
        for status in [429, 500, 502, 503] {
            assert_eq!(
                verdict(status, ""),
                Verdict::Retryable,
                "{status} is Slack asking for the message again later"
            );
        }
    }

    #[test]
    fn test_other_client_errors_are_permanent() {
        for status in [400, 401, 403, 404] {
            assert_eq!(
                verdict(status, ""),
                Verdict::Permanent,
                "{status} will not change on a retry"
            );
        }
    }

    #[test]
    fn test_a_complete_request_json_names_the_thread_to_answer_in() {
        let resolved = destination(
            r#"{"channel_id":"C123","reply_thread_ts":"1700000000.000100","text":"status?"}"#,
        );

        assert!(
            matches!(
                &resolved,
                Some(where_to)
                    if where_to.channel == "C123"
                        && where_to.thread == "1700000000.000100"
            ),
            "both destination fields are present: {resolved:?}"
        );
    }

    /// Every shape that names nowhere to post. Present-and-empty is in here
    /// deliberately: it is the one a bare presence check would let through, and
    /// it would be found at the vendor, a request later.
    #[test]
    fn test_no_unpostable_request_json_resolves_a_destination() {
        for stored in [
            r#"{"channel_id":"","reply_thread_ts":"1700000000.000100"}"#,
            r#"{"channel_id":"C123","reply_thread_ts":""}"#,
            r#"{"channel_id":"C123"}"#,
            r#"{"reply_thread_ts":"1700000000.000100"}"#,
            r#"{"channel_id":42,"reply_thread_ts":"1700000000.000100"}"#,
            r#"{"channel_id":null,"reply_thread_ts":"1700000000.000100"}"#,
            "{}",
            "not json",
        ] {
            assert!(
                destination(stored).is_none(),
                "`{stored}` names nowhere to put an answer"
            );
        }
    }

    /// The answer is model output, so the body has to carry whatever a run
    /// produced — through `serde`, never through interpolation.
    #[test]
    fn test_the_body_escapes_an_answer_carrying_json_punctuation() {
        let answer = "she said \"yes\"\nand {\"ok\": false}";
        let round_tripped: Option<serde_json::Value> = serde_json::to_vec(&Message {
            channel: "C123",
            thread_ts: "1700000000.000100",
            text: answer,
        })
        .ok()
        .and_then(|body| serde_json::from_slice(&body).ok());

        assert_eq!(
            round_tripped
                .as_ref()
                .and_then(|message| message.get("text"))
                .and_then(serde_json::Value::as_str),
            Some(answer),
            "what serde wrote, serde reads back whole"
        );
    }
}
