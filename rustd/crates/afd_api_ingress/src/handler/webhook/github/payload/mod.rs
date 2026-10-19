//! The GitHub payload types: only the fields a decision or a digest reads.
//!
//! A field not declared here is never read, so a delivery missing one still
//! reads. The decisions over these fields live in the parent module.

use serde::Deserialize;
use serde::de::Deserializer;
use serde_json::value::RawValue;

/// The `x-github-event` word for a workflow run.
const EVENT_WORKFLOW_RUN: &str = "workflow_run";

/// The `x-github-event` word for a pull request.
const EVENT_PULL_REQUEST: &str = "pull_request";

/// What every delivery carries beside its event's own body: the two fields the
/// App ingress routes on.
///
/// Both are optional, as GitHub sends them: a manual hook carries no
/// installation, and an organisation-level event carries no repository.
#[derive(Debug, Deserialize)]
struct Envelope {
    installation: Option<Installation>,
    repository: Option<Repository>,
}

/// The installation an App delivery was sent for.
#[derive(Debug, Deserialize)]
struct Installation {
    id: u64,
}

/// The repository a delivery is about.
#[derive(Debug, Deserialize)]
struct Repository {
    full_name: Option<String>,
}

/// A `pull_request` delivery's own body.
///
/// `action`, `number` and `pull_request` are required, so a body that is not a
/// pull-request delivery fails to read and answers `UZ-WH-002`. The action is
/// kept as GitHub's word: the App route accepts every action, so a word GitHub
/// adds next year reads rather than refusing a signed delivery.
#[derive(Debug, Deserialize)]
pub(super) struct PullRequestEvent {
    pub(super) action: String,
    pub(super) number: u64,
    pub(super) pull_request: PullRequest,
}

/// The pull request a `pull_request` delivery describes.
#[derive(Debug, Deserialize)]
pub(super) struct PullRequest {
    pub(super) title: Option<String>,
    pub(super) html_url: Option<String>,
    pub(super) state: Option<String>,
    pub(super) draft: Option<bool>,
    pub(super) user: Option<Actor>,
    pub(super) head: Branch,
    pub(super) base: Branch,
}

/// Who opened a pull request.
#[derive(Debug, Deserialize)]
pub(super) struct Actor {
    pub(super) login: String,
}

/// One end of a pull request.
#[derive(Debug, Deserialize)]
pub(super) struct Branch {
    #[serde(rename = "ref")]
    pub(super) reference: String,
    pub(super) sha: String,
}

/// A `workflow_run` delivery's own body.
#[derive(Debug, Deserialize)]
pub(super) struct WorkflowRunEvent {
    pub(super) action: RunAction,
    #[serde(deserialize_with = "run_or_default")]
    pub(super) workflow_run: WorkflowRun,
}

/// The three actions GitHub sends a `workflow_run` delivery under.
///
/// Every one is a variant so an action outside them fails the read rather than
/// passing for one of them.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RunAction {
    Completed,
    InProgress,
    Requested,
}

/// The `workflow_run` fields the digest carries.
///
/// Every field is `#[serde(default)]`: a delivery missing one is still a
/// delivery, and refusing it would make this daemon stricter than the sender
/// it serves.
#[derive(Debug, Default, Deserialize)]
pub(super) struct WorkflowRun {
    #[serde(default)]
    pub(super) html_url: String,
    #[serde(default)]
    pub(super) head_sha: String,
    #[serde(default)]
    pub(super) conclusion: String,
    #[serde(default)]
    pub(super) head_branch: String,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default = "one")]
    pub(super) run_attempt: i64,
    #[serde(default)]
    pub(super) id: i64,
}

/// A run with no stated attempt is its first — `orelse 1`, mirrored.
pub(super) const fn one() -> i64 {
    1
}

/// The run, or an empty one when it does not read as [`WorkflowRun`].
///
/// The `workflow_run` object must be present, but a run whose fields are the
/// wrong type reads as empty — its conclusion is then not `failure`, and it is
/// dropped rather than refused.
fn run_or_default<'de, D: Deserializer<'de>>(deserializer: D) -> Result<WorkflowRun, D::Error> {
    let raw = Box::<RawValue>::deserialize(deserializer)?;
    Ok(serde_json::from_str(raw.get()).unwrap_or_default())
}

/// The body an event this file has a rule for.
#[derive(Debug)]
pub(super) enum Body {
    PullRequest(PullRequestEvent),
    WorkflowRun(WorkflowRunEvent),
    /// An event kind this daemon serves no rule for; its body is not read.
    Other,
}

/// One delivery, read once: the routing fields and its event's own body.
#[derive(Debug)]
pub(crate) struct Delivery {
    envelope: Envelope,
    pub(super) body: Body,
}

impl Delivery {
    /// Reads `body` as the `event` its `x-github-event` header names.
    ///
    /// The delivery's kind is GitHub's word for it, never inferred from the
    /// body's shape.
    ///
    /// An event with no rule here is read no further than its envelope, so
    /// its own body cannot refuse it: it is classified as unsupported.
    ///
    /// # Errors
    /// Returns the deserialization failure for a body that is not a JSON
    /// object, or a `pull_request` or `workflow_run` body that is not the event
    /// its header claims. A malformed body is the sender's fault and answers
    /// `UZ-WH-002`; it is never silently treated as an unsupported event.
    pub(crate) fn read(event: &str, body: &[u8]) -> Result<Self, serde_json::Error> {
        let envelope = serde_json::from_slice(body)?;
        let body = match event {
            EVENT_PULL_REQUEST => Body::PullRequest(serde_json::from_slice(body)?),
            EVENT_WORKFLOW_RUN => Body::WorkflowRun(serde_json::from_slice(body)?),
            _ => Body::Other,
        };
        Ok(Self { envelope, body })
    }

    /// The installation an App delivery was sent for.
    pub(crate) fn installation(&self) -> Option<u64> {
        self.envelope
            .installation
            .as_ref()
            .map(|install| install.id)
    }

    /// The `owner/name` of the repository the delivery is about.
    pub(crate) fn repository(&self) -> Option<&str> {
        self.envelope
            .repository
            .as_ref()
            .and_then(|repository| repository.full_name.as_deref())
    }
}

#[cfg(test)]
mod tests;
