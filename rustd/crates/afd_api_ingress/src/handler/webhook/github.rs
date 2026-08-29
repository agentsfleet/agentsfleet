//! A GitHub delivery, classified and reduced to the digest a fleet reasons over.
//!
//! # The digest is a contract, not a convenience
//!
//! `normalizer/github.zig` says it outright — *"field names match the spec's
//! `request_json` contract; the agent reasons over these directly"*. A fleet's
//! authored prose reads `conclusion` and `run_url`, so these field names are as
//! load-bearing as any wire type. Forwarding GitHub's raw eighty-field payload
//! instead would break that prose, spend the tokens to do it, and hand an
//! attacker-influenced document straight to a model (RULE PRI).
//!
//! # What was ported and what was replaced
//!
//! The DECISIONS are ported: which events wake a fleet, which are deliberately
//! ignored, and which fields the digest carries. The MECHANISM is not.
//! `github.zig` and `github_app.zig` walk the payload with five helpers —
//! `objectField`, `nestedStringField`, `stringField`, `integerField`,
//! `boolField` — and a `switch` per field, because Zig has no `derive`. That is
//! a Zig constraint, not a design (RULE PORT). Here `octocrab`'s webhook types
//! answer the same questions from a type declaration, and the helpers are gone.
//!
//! `octocrab` was already a workspace dependency for App JWT minting, so the
//! typed payloads cost one direct dependency and no transitive crate at all.
//! Its `workflow_run` body is still `serde_json::Value` in 0.54.1 — upstream
//! labels webhook support beta — so that one is read through a narrow
//! `Deserialize` struct rather than by indexing.
//!
//! # Two routes, two policies, and they really do differ
//!
//! The manual per-fleet route and the App ingress apply DIFFERENT pull-request
//! rules, and the Zig states that only by having two files. It is named here
//! instead — see [`Policy`] — because a divergence a reader has to discover by
//! opening a second file is one that gets 'fixed' by somebody who did not.

use afd_core::clock::UnixMillis;
use octocrab::models::webhook_events::payload::{
    PullRequestWebhookEventAction, WorkflowRunWebhookEventAction,
};
use octocrab::models::webhook_events::{WebhookEvent, WebhookEventPayload};
use serde::{Deserialize, Serialize};

/// The reason a delivery this daemon understands is deliberately not ingested.
///
/// Every one of these is a real, correctly-signed delivery. They are answered
/// 2xx and dropped, because a sender retrying them would change nothing.
const REASON_NON_COMPLETED_ACTION: &str = "non_completed_action";
/// See [`REASON_NON_COMPLETED_ACTION`].
const REASON_NON_FAILURE_CONCLUSION: &str = "non_failure_conclusion";
/// See [`REASON_NON_COMPLETED_ACTION`].
const REASON_MISSING_REPOSITORY: &str = "missing_repository";
/// See [`REASON_NON_COMPLETED_ACTION`].
const REASON_REPAIR_BRANCH: &str = "repair_branch";
/// See [`REASON_NON_COMPLETED_ACTION`].
const REASON_UNINTERESTING_ACTION: &str = "uninteresting_action";

/// The workflow conclusion that wakes a fleet.
///
/// One value, not a set: a fleet is woken to investigate a FAILURE. A green
/// run is the case this daemon exists to avoid spending a model on.
const CONCLUSION_FAILURE: &str = "failure";

/// Which route's rules to apply.
///
/// The two are not interchangeable and the difference is entirely in the
/// pull-request arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Policy {
    /// `POST /v1/webhooks/{fleet_id}/github` — the fleet is named in the URL.
    ///
    /// Accepts the four pull-request actions that mean "there is new code to
    /// look at": opened, reopened, synchronize, `ready_for_review`. The fleet was
    /// addressed directly, so the narrow set is what keeps a label edit or an
    /// assignment from spending a model.
    Manual,
    /// `POST /v1/ingress/github` — the App fans one delivery out to subscribers.
    ///
    /// Accepts EVERY pull-request action except a repair branch's.
    /// Deliberately wider than [`Policy::Manual`]: a subscription already
    /// narrowed this delivery by repository and event, so the action filter
    /// would be a second, invisible narrowing on top of one the fleet author
    /// wrote. `github_app.zig` behaves this way; it just never says so.
    AppIngress,
}

/// What the ingress should do with one delivery.
#[derive(Debug)]
pub(crate) enum Ingest {
    /// Wake the fleet, with this as the event's `request_json`.
    Accept(String),
    /// A real delivery this daemon deliberately drops, and why.
    Ignore(&'static str),
    /// An event kind this daemon serves no rule for.
    ///
    /// Distinct from [`Ingest::Ignore`]: that is a decision, this is an
    /// absence, and only one of them is a reason to add code.
    Unsupported,
}

/// The `workflow_run` fields the digest carries.
///
/// A narrow reader over octocrab's untyped `workflow_run` value. Every field is
/// `#[serde(default)]` for the same reason the Zig writes `orelse ""`: a
/// delivery missing one is still a delivery, and refusing it would make this
/// daemon stricter than the sender it serves.
#[derive(Debug, Default, Deserialize)]
struct WorkflowRunBody {
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    head_sha: String,
    #[serde(default)]
    conclusion: String,
    #[serde(default)]
    head_branch: String,
    #[serde(default)]
    name: String,
    #[serde(default = "one")]
    run_attempt: i64,
    #[serde(default)]
    id: i64,
}

/// A run with no stated attempt is its first — `orelse 1`, mirrored.
const fn one() -> i64 {
    1
}

/// The flat object a `workflow_run` becomes on the stream.
///
/// Field names and order are `normalizer/github.zig`'s `Normalized`, kept
/// exactly: a fleet's prose reads them.
#[derive(Debug, Serialize)]
struct WorkflowRunDigest<'a> {
    run_url: &'a str,
    head_sha: &'a str,
    conclusion: &'a str,
    repo: &'a str,
    attempt: i64,
    run_id: i64,
    head_branch: &'a str,
    workflow_name: &'a str,
    received_at: &'a str,
}

/// The flat object a `pull_request` becomes on the stream.
///
/// `github_app.zig`'s `PullRequest`, field for field.
#[derive(Debug, Serialize)]
struct PullRequestDigest<'a> {
    action: &'a str,
    repo: &'a str,
    number: u64,
    title: &'a str,
    url: &'a str,
    state: &'a str,
    draft: bool,
    author: &'a str,
    head_ref: &'a str,
    base_ref: &'a str,
    head_sha: &'a str,
    received_at: &'a str,
}

/// Classifies one delivery under `policy`.
///
/// `event` is the `x-github-event` header's value — the delivery's kind is
/// GitHub's word for it, never inferred from the body's shape.
///
/// # Errors
/// Returns the deserialization failure for a body that is not the event its
/// header claims. A malformed body is the sender's fault and answers
/// `UZ-WH-002`; it is never silently treated as an unsupported event.
pub(crate) fn classify(
    policy: Policy,
    event: &str,
    body: &[u8],
    received_at: UnixMillis,
) -> Result<Ingest, serde_json::Error> {
    let delivery = WebhookEvent::try_from_header_and_body(event, body)?;
    let stamp = receipt_stamp(received_at);
    // Read once from the COMMON half, before any per-event match: both digests
    // carry it and both policies refuse without it.
    let Some(repository) = delivery
        .repository
        .as_ref()
        .and_then(|repository| repository.full_name.clone())
    else {
        return Ok(Ingest::Ignore(REASON_MISSING_REPOSITORY));
    };

    Ok(match delivery.specific {
        WebhookEventPayload::WorkflowRun(run) => workflow_run(&run, &repository, &stamp),
        WebhookEventPayload::PullRequest(pull) => pull_request(policy, &pull, &repository, &stamp),
        _ => Ingest::Unsupported,
    })
}

/// The `workflow_run` arm — identical under both policies.
fn workflow_run(
    payload: &octocrab::models::webhook_events::payload::WorkflowRunWebhookEventPayload,
    repository: &str,
    received_at: &str,
) -> Ingest {
    if payload.action != WorkflowRunWebhookEventAction::Completed {
        return Ingest::Ignore(REASON_NON_COMPLETED_ACTION);
    }
    let run: WorkflowRunBody =
        serde_json::from_value(payload.workflow_run.clone()).unwrap_or_default();

    // Ordered as `github_app.zig` orders it: the repair branch is checked
    // BEFORE the conclusion, so a fleet's own failed repair is reported as the
    // loop it is rather than as an ordinary failure.
    if is_repair_branch(&run.head_branch) {
        return Ingest::Ignore(REASON_REPAIR_BRANCH);
    }
    if run.conclusion != CONCLUSION_FAILURE {
        return Ingest::Ignore(REASON_NON_FAILURE_CONCLUSION);
    }

    let digest = WorkflowRunDigest {
        run_url: &run.html_url,
        head_sha: &run.head_sha,
        conclusion: &run.conclusion,
        repo: repository,
        attempt: run.run_attempt,
        run_id: run.id,
        head_branch: &run.head_branch,
        workflow_name: &run.name,
        received_at,
    };
    encoded(&digest)
}

/// The `pull_request` arm — where the two policies part company.
fn pull_request(
    policy: Policy,
    payload: &octocrab::models::webhook_events::payload::PullRequestWebhookEventPayload,
    repository: &str,
    received_at: &str,
) -> Ingest {
    let pull = &payload.pull_request;
    let head_ref = pull.head.ref_field.as_str();
    if is_repair_branch(head_ref) {
        return Ingest::Ignore(REASON_REPAIR_BRANCH);
    }
    if policy == Policy::Manual && !wakes_a_fleet(&payload.action) {
        return Ingest::Ignore(REASON_UNINTERESTING_ACTION);
    }

    // Both words are taken from the same `serde` attribute that PARSED them,
    // rather than from a match this file would have to keep current against an
    // upstream `#[non_exhaustive]` enum. A digest cannot then spell an action
    // differently from the delivery it describes.
    let action = wire_word(&payload.action);
    let state = pull.state.as_ref().map(wire_word).unwrap_or_default();
    let url = pull
        .html_url
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();

    let digest = PullRequestDigest {
        action: &action,
        repo: repository,
        number: payload.number,
        title: pull.title.as_deref().unwrap_or_default(),
        url: &url,
        state: &state,
        draft: pull.draft.unwrap_or(false),
        author: pull.user.as_ref().map_or("", |user| user.login.as_str()),
        head_ref,
        base_ref: pull.base.ref_field.as_str(),
        head_sha: pull.head.sha.as_str(),
        received_at,
    };
    encoded(&digest)
}

/// A `serde`-renamed enum's wire spelling.
///
/// Goes through the serializer rather than a `match` for a reason the upstream
/// types force: both enums this is used on are `#[non_exhaustive]`, so a match
/// needs a catch-all arm, and a catch-all silently spells a NEW upstream
/// variant as the empty string. Asking serde asks the one attribute that
/// already decided the spelling on the way in.
fn wire_word<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|encoded| encoded.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The four pull-request actions that mean there is new code to look at.
///
/// `github_filter.zig`'s set, and it is the MANUAL route's alone.
const fn wakes_a_fleet(action: &PullRequestWebhookEventAction) -> bool {
    matches!(
        action,
        PullRequestWebhookEventAction::Opened
            | PullRequestWebhookEventAction::Reopened
            | PullRequestWebhookEventAction::Synchronize
            | PullRequestWebhookEventAction::ReadyForReview
    )
}

/// Whether a branch is the repairer's own output.
///
/// Traffic on one is the crew hearing itself: waking a fleet on its own failed
/// repair sets it investigating what it just wrote, one approval card per
/// cycle.
fn is_repair_branch(reference: &str) -> bool {
    reference.starts_with(afd_gate::policy::repair::PREFIX)
}

/// The digest, encoded, or the ignore a body that will not encode earns.
fn encoded<T: Serialize>(digest: &T) -> Ingest {
    serde_json::to_string(digest).map_or(Ingest::Ignore(REASON_MISSING_REPOSITORY), Ingest::Accept)
}

/// The receipt stamp a digest carries, as RFC 3339 with a `Z` and no fraction.
///
/// `formatRfc3339` computes the civil date from epoch seconds by hand —
/// `getEpochDay`, `calculateYearDay`, `calculateMonthDay`, then a `bufPrint`
/// with a `@panic` for a buffer it sized itself. All of that is Zig's stdlib
/// having no date type. A timestamp this daemon cannot represent falls back to
/// the epoch rather than failing a delivery over a clock.
fn receipt_stamp(received_at: UnixMillis) -> String {
    jiff::Timestamp::from_second(received_at.as_seconds())
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

#[cfg(test)]
mod tests;
