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
//! `post.zig` walks `std.json.Value` with a `strField` helper because Zig has
//! no derive. Here the destination (`afd_connector::slack::Thread`, the one
//! type the mention producer writes and this reads) and Slack's answer are each
//! a struct with `Deserialize` on it, so the shape is stated once and the
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

use afd_connector::slack::Thread;
use afd_connector::{Grants, Provider};
use afd_core::id::Uuid7;
use afd_crypto::secret::SecretBytes;
use afd_dragonfly::OutboundDelivery;
use serde::{Deserialize, Serialize};

use crate::poster::{Deliver, Verdict};

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
    /// deployment and a loopback in a test — the same seam `post.zig` carries
    /// for the same reason. The client is shared with the rest of the
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

    /// Both inputs, the address first and from the job alone.
    ///
    /// Answers a verdict directly on failure — see the module note on why
    /// nothing here is an error.
    async fn inputs(&self, job: &OutboundDelivery) -> Result<Inputs, Verdict> {
        let destination = destination(job)?;
        let Ok(workspace) = Uuid7::parse(&job.workspace_id) else {
            // An identifier this daemon queued that will not parse is this
            // build's own bug, not a transient: retrying re-runs the parse.
            return Err(failed(job, "identifier_unparseable", Verdict::Permanent));
        };

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

    /// The POST itself, with no pool connection held — see the module note.
    async fn post(&self, job: &OutboundDelivery, inputs: &Inputs) -> Verdict {
        let Ok(token) = std::str::from_utf8(inputs.token.expose()) else {
            return failed(job, REASON_TOKEN_LOAD_FAILED, Verdict::Permanent);
        };
        let body = serde_json::to_vec(&Message {
            channel: &inputs.destination.channel_id,
            thread_ts: &inputs.destination.thread_ts,
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
    destination: Thread,
    /// Still wrapped, so it zeroes on drop — see `Grants::bot_token`.
    token: SecretBytes,
}

/// Where the answer goes, read from the job's recorded address.
///
/// Not JSON, missing a field, or naming an empty channel or thread — one
/// answer for all of them, because a caller does the same thing with each: the
/// job names nowhere this poster can post, and no retry changes that. Nothing
/// has been read or requested when it answers.
fn destination(job: &OutboundDelivery) -> Result<Thread, Verdict> {
    Thread::parse(&job.destination)
        .ok_or_else(|| failed(job, REASON_ADDRESS_UNREADABLE, Verdict::Permanent))
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

    /// A job carrying `address`, with every other field well formed.
    fn job(address: &str) -> OutboundDelivery {
        OutboundDelivery {
            id: afd_dragonfly::streams::EventId::of("1700000000001-0"),
            provider: Provider::Slack.id().to_owned(),
            destination: address.to_owned(),
            workspace_id: "0199a0b0-0000-7000-8000-000000000001".to_owned(),
            fleet_id: "0199a0b0-0000-7000-8000-000000000002".to_owned(),
            event_id: "1700000000000-0".to_owned(),
            answer: "the answer".to_owned(),
        }
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

    /// Every shape that names nowhere to post. Present-and-empty is in here
    /// deliberately: it is the one a bare presence check would let through, and
    /// it would be found at the vendor, a request later.
    const UNPOSTABLE: [&str; 9] = [
        r#"{"channel_id":"","thread_ts":"1700000000.000100"}"#,
        r#"{"channel_id":"C123","thread_ts":""}"#,
        r#"{"channel_id":"C123"}"#,
        r#"{"thread_ts":"1700000000.000100"}"#,
        r#"{"channel_id":42,"thread_ts":"1700000000.000100"}"#,
        r#"{"channel_id":null,"thread_ts":"1700000000.000100"}"#,
        r#"{"channel_id":"C123","reply_thread_ts":"1700000000.000100"}"#,
        "{}",
        "not json",
    ];

    /// Dimension 3.2 — an address naming nowhere is a permanent verdict, and it
    /// is reached from the job alone: nothing has been read or requested.
    #[test]
    fn unreadable_address_is_permanent_without_a_request() {
        for stored in UNPOSTABLE {
            assert!(
                matches!(destination(&job(stored)), Err(Verdict::Permanent)),
                "`{stored}` must be refused before any request is built"
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
