//! What a signature-verified delivery is answered with.
//!
//! # Why these are here rather than beside their handlers
//!
//! Every one of these is a PUBLIC response body. A sender reads it from its own
//! delivery log, the dashboard renders it, and `public/openapi.json` declares
//! it — three readers that must agree on one shape. Defining them beside the
//! axum handler puts that shape somewhere only the daemon can see, which is the
//! arrangement this crate exists to prevent: `afd_api_tenant`, `_operator` and
//! `_runner` all read their response types from here, and the ingress plane was
//! the sole holdout.
//!
//! # The two digests are stream payloads, not HTTP bodies
//!
//! [`WorkflowRunDigest`] and [`PullRequestDigest`] are what a verified GitHub
//! delivery BECOMES on the fleet's event stream. They are wire all the same, and
//! more strictly so than a response: a fleet's prose reads these field names, so
//! renaming one silently changes what every fleet sees. `normalizer/github.zig`
//! is the shape of record and the field order is part of it.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::Serialize;

/// What an accepted delivery is answered with.
///
/// `202`, and the event id, so a provider's delivery log carries the identifier
/// an operator can search the fleet's history by. A replayed delivery answers
/// the FIRST attempt's id rather than a new one — that is the whole point of
/// the at-most-once claim, and a sender comparing two responses should see the
/// same event both times.
#[derive(Debug, Clone, Serialize)]
pub struct Accepted<'a> {
    /// The event the fleet will run, or already ran.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
    /// Whether an earlier delivery already produced it.
    ///
    /// Reported rather than hidden: a provider debugging a duplicate wants to
    /// know this daemon SAW the repeat and declined to run twice, which is a
    /// different fact from the delivery having been lost.
    pub replayed: bool,
}

/// What a delivery this daemon deliberately dropped is answered with.
///
/// `200` and a reason, never a 4xx. Every one of these is a real,
/// correctly-signed delivery that simply does not wake this fleet — a green
/// build, a label edit, a paused fleet. Answering an error would put it in the
/// sender's retry queue forever, and retrying changes none of them.
#[derive(Debug, Clone, Serialize)]
pub struct Ignored<'a> {
    /// Which rule dropped it.
    #[serde(borrow)]
    pub ignored: Cow<'a, str>,
}

/// What an accepted App delivery is answered with.
///
/// Wider than [`Accepted`] because one App delivery is many appends: a sender
/// debugging its integration wants to know how many fleets this installation
/// actually woke, which is the number no single event id can show.
#[derive(Debug, Clone, Serialize)]
pub struct FannedOut {
    /// How many fleets subscribed to this delivery.
    pub matched: usize,
    /// How many of them this delivery actually appended an event for.
    ///
    /// Lower than `matched` when a fleet already ran this delivery — the claim
    /// is per fleet, so a retry that reaches a wider set than the first attempt
    /// appends only for the fleets that had not seen it.
    pub enqueued: usize,
}

/// What a `ping` is answered with.
#[derive(Debug, Clone, Serialize)]
pub struct Pong<'a> {
    /// The pong marker this deployment answers with.
    #[serde(borrow)]
    pub status: Cow<'a, str>,
}

/// What a gate resolved through the approval callback is answered with.
///
/// Deliberately NOT [`crate::approval::ResolvedResponse`]. That is the
/// dashboard's shape, carrying the gate id, the outcome and who decided it; this
/// is the shape the callback sender is owed, and the two differ because their
/// readers do. A sender gets back what it sent plus a marker, and nothing about
/// who else may have answered the gate first.
#[derive(Debug, Clone, Serialize)]
pub struct Resolved<'a> {
    /// The resolved marker this route answers with.
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    /// The gate that was answered.
    #[serde(borrow)]
    pub action_id: Cow<'a, str>,
    /// The answer it was given.
    #[serde(borrow)]
    pub decision: Cow<'a, str>,
}

/// What an accepted schedule fire is answered with.
#[derive(Debug, Clone, Serialize)]
pub struct Fired<'a> {
    /// The event the fleet will run, or already ran.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
    /// Whether an earlier attempt already produced it.
    pub replayed: bool,
}

/// The echo a connector handshake is answered with.
///
/// A one-key map rather than a struct because the KEY is provider data, and a
/// struct would fix it at compile time to whichever vendor was ported first.
#[derive(Debug, Clone, Serialize)]
pub struct EchoAnswer<'a> {
    /// The provider's own challenge field, echoed under its own name.
    #[serde(flatten, borrow)]
    pub field: BTreeMap<&'a str, &'a str>,
}

/// The flat object a `workflow_run` becomes on the stream.
///
/// Field names and order are `normalizer/github.zig`'s `Normalized`, kept
/// exactly: a fleet's prose reads them.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowRunDigest<'a> {
    /// The run's page on the forge.
    #[serde(borrow)]
    pub run_url: Cow<'a, str>,
    /// The commit the run was taken at.
    #[serde(borrow)]
    pub head_sha: Cow<'a, str>,
    /// How the run ended.
    #[serde(borrow)]
    pub conclusion: Cow<'a, str>,
    /// The repository it belongs to.
    #[serde(borrow)]
    pub repo: Cow<'a, str>,
    /// Which attempt this was.
    pub attempt: i64,
    /// The forge's own id for the run.
    pub run_id: i64,
    /// The branch the run was taken on.
    #[serde(borrow)]
    pub head_branch: Cow<'a, str>,
    /// The workflow that produced it.
    #[serde(borrow)]
    pub workflow_name: Cow<'a, str>,
    /// When this daemon accepted the delivery.
    #[serde(borrow)]
    pub received_at: Cow<'a, str>,
}

/// The flat object a `pull_request` becomes on the stream.
///
/// `github_app.zig`'s `PullRequest`, field for field.
#[derive(Debug, Clone, Serialize)]
pub struct PullRequestDigest<'a> {
    /// What happened to the pull request.
    #[serde(borrow)]
    pub action: Cow<'a, str>,
    /// The repository it belongs to.
    #[serde(borrow)]
    pub repo: Cow<'a, str>,
    /// Its number within the repository.
    pub number: u64,
    /// Its title, as the author wrote it.
    #[serde(borrow)]
    pub title: Cow<'a, str>,
    /// Its page on the forge.
    #[serde(borrow)]
    pub url: Cow<'a, str>,
    /// Whether it is open or closed.
    #[serde(borrow)]
    pub state: Cow<'a, str>,
    /// Whether it is still a draft.
    pub draft: bool,
    /// Who opened it.
    #[serde(borrow)]
    pub author: Cow<'a, str>,
    /// The branch being merged FROM.
    #[serde(borrow)]
    pub head_ref: Cow<'a, str>,
    /// The branch being merged INTO.
    #[serde(borrow)]
    pub base_ref: Cow<'a, str>,
    /// The commit the run was taken at.
    #[serde(borrow)]
    pub head_sha: Cow<'a, str>,
    /// When this daemon accepted the delivery.
    #[serde(borrow)]
    pub received_at: Cow<'a, str>,
}
