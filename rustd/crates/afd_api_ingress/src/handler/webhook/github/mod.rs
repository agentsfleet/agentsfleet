//! A GitHub delivery, classified and reduced to the digest a fleet reasons over.
//!
//! # The digest's field names are load-bearing
//!
//! A fleet's authored prose reads `conclusion` and `run_url` directly off the
//! event's `request_json`, so these field names are as load-bearing as any
//! wire type. Forwarding GitHub's raw eighty-field payload instead would break
//! that prose, spend the tokens to do it, and hand an attacker-influenced
//! document straight to a model (RULE PRI).
//!
//! # Decisions here, field reading in `serde`
//!
//! This file owns the DECISIONS: which events wake a fleet, which are
//! deliberately ignored, and which fields the digest carries. Reading the
//! fields is the job of the `Deserialize` types below, each declaring only the
//! fields a decision or a digest reads, so the answer to "what does this read"
//! is a type declaration rather than a hand-written walk over the payload.
//!
//! Declaring only those fields is also what keeps an unread one from refusing
//! a delivery: a payload GitHub grows, or an actor missing a URL field nobody
//! reads, still reads.
//!
//! # Two routes, two policies, and they really do differ
//!
//! The manual per-fleet route and the App ingress apply DIFFERENT pull-request
//! rules. The difference is named here — see [`Policy`] — because a divergence
//! a reader has to discover by opening a second file is one that gets 'fixed'
//! by somebody who did not.

/// The two flat objects a verified delivery becomes on the stream. Stream
/// payloads rather than HTTP bodies, and wire all the same: a fleet's prose
/// reads these field names, so `afd_wire` owns them.
use afd_api_wire::ingress::{PullRequestDigest, WorkflowRunDigest};
use serde::Serialize;

pub(crate) use payload::Delivery;
use payload::{Body, PullRequestEvent, RunAction, WorkflowRunEvent};

mod payload;

/// The four pull-request actions that mean there is new code to look at.
///
/// The MANUAL route's set alone: [`Policy::AppIngress`] accepts every action
/// except a repair branch's.
const ACTIONS_WITH_NEW_CODE: [&str; 4] = ["opened", "reopened", "synchronize", "ready_for_review"];

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
    /// wrote.
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

impl Delivery {
    /// Classifies this delivery under `policy`.
    pub(crate) fn classify(&self, policy: Policy, received_at: jiff::Timestamp) -> Ingest {
        // Read once from the COMMON half, before any per-event match: both
        // digests carry it and both policies refuse without it.
        let Some(repository) = self.repository() else {
            return Ingest::Ignore(REASON_MISSING_REPOSITORY);
        };
        match &self.body {
            Body::WorkflowRun(run) => workflow_run(run, repository, received_at),
            Body::PullRequest(pull) => pull_request(policy, pull, repository, received_at),
            Body::Other => Ingest::Unsupported,
        }
    }
}

/// Classifies one delivery under `policy`.
///
/// `event` is the `x-github-event` header's value.
///
/// # Errors
/// As [`Delivery::read`].
pub(crate) fn classify(
    policy: Policy,
    event: &str,
    body: &[u8],
    received_at: jiff::Timestamp,
) -> Result<Ingest, serde_json::Error> {
    Delivery::read(event, body).map(|delivery| delivery.classify(policy, received_at))
}

/// The `workflow_run` arm — identical under both policies.
fn workflow_run(
    event: &WorkflowRunEvent,
    repository: &str,
    received_at: jiff::Timestamp,
) -> Ingest {
    if event.action != RunAction::Completed {
        return Ingest::Ignore(REASON_NON_COMPLETED_ACTION);
    }
    let run = &event.workflow_run;

    // The repair branch is checked BEFORE the conclusion, so a fleet's own
    // failed repair is reported as the loop it is rather than as an ordinary
    // failure.
    if is_repair_branch(&run.head_branch) {
        return Ingest::Ignore(REASON_REPAIR_BRANCH);
    }
    if run.conclusion != CONCLUSION_FAILURE {
        return Ingest::Ignore(REASON_NON_FAILURE_CONCLUSION);
    }

    let digest = WorkflowRunDigest {
        run_url: run.html_url.as_str().into(),
        head_sha: run.head_sha.as_str().into(),
        conclusion: run.conclusion.as_str().into(),
        repo: repository.into(),
        attempt: run.run_attempt,
        run_id: run.id,
        head_branch: run.head_branch.as_str().into(),
        workflow_name: run.name.as_str().into(),
        received_at,
    };
    encoded(&digest)
}

/// The `pull_request` arm — where the two policies part company.
fn pull_request(
    policy: Policy,
    event: &PullRequestEvent,
    repository: &str,
    received_at: jiff::Timestamp,
) -> Ingest {
    if is_repair_branch(&event.pull_request.head.reference) {
        return Ingest::Ignore(REASON_REPAIR_BRANCH);
    }
    if policy == Policy::Manual && !ACTIONS_WITH_NEW_CODE.contains(&event.action.as_str()) {
        return Ingest::Ignore(REASON_UNINTERESTING_ACTION);
    }
    pull_request_digest(event, repository, received_at)
}

/// The digest an accepted `pull_request` delivery is admitted as.
///
/// `action` and `state` are GitHub's own words, carried as delivered.
fn pull_request_digest(
    event: &PullRequestEvent,
    repository: &str,
    received_at: jiff::Timestamp,
) -> Ingest {
    let pull = &event.pull_request;
    let digest = PullRequestDigest {
        action: event.action.as_str().into(),
        repo: repository.into(),
        number: event.number,
        title: pull.title.as_deref().unwrap_or_default().into(),
        url: pull.html_url.as_deref().unwrap_or_default().into(),
        state: pull.state.as_deref().unwrap_or_default().into(),
        draft: pull.draft.unwrap_or(false),
        author: pull
            .user
            .as_ref()
            .map_or("", |user| user.login.as_str())
            .into(),
        head_ref: pull.head.reference.as_str().into(),
        base_ref: pull.base.reference.as_str().into(),
        head_sha: pull.head.sha.as_str().into(),
        received_at,
    };
    encoded(&digest)
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

#[cfg(test)]
mod tests;
