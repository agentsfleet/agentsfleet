//! The path-parameter shapes the route templates address resources by.
//!
//! # Why these are types and not literals at each annotation
//!
//! `#[utoipa::path(params(…))]` takes a parameter's NAME as a literal token — a
//! `const` is refused by the macro — so twelve routes under
//! `/v1/workspaces/{workspace_id}` spelled that name twelve times, and eleven
//! more spelled it beside `{fleet_id}`. Repeated literals across call sites are
//! what drifts (RULE UFS), and here the drift would be silent: a parameter
//! documented under a name the template does not carry describes a route
//! nobody can call.
//!
//! `IntoParams` moves the name from a literal to a FIELD IDENTIFIER, so each
//! shape is declared once and every route addressed that way refers to it.
//! Renaming a parameter becomes a compile-time rename rather than a search.
//!
//! # These describe the template, not the handler
//!
//! A handler extracts what it needs and no more; the template is what a caller
//! must fill in. So the shapes here mirror [`crate::route::RouteMeta::template`]
//! — that is what the coverage gate compares against, and what a reader of the
//! document has to satisfy.

use utoipa::IntoParams;

/// A resource addressed by its own identifier.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Id {
    /// Resource identifier (version 7 universally unique identifier, or UUID).
    pub id: String,
}

/// Anything under one workspace.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Workspace {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
}

/// Anything under one fleet inside one workspace.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Fleet {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Fleet identifier (version 7 universally unique identifier, or UUID).
    pub fleet_id: String,
}

/// A fleet addressed without its workspace — the signed-delivery surface.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct FleetOnly {
    /// Fleet identifier (version 7 universally unique identifier, or UUID).
    pub fleet_id: String,
}

/// One webhook source of a fleet addressed without its workspace.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct FleetSource {
    /// Fleet identifier (version 7 universally unique identifier, or UUID).
    pub fleet_id: String,
    /// The `source` of the fleet's webhook trigger, such as `linear`.
    pub source: String,
}

/// One third-party provider.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Provider {
    /// Provider identifier, such as `slack` or `github`.
    pub provider: String,
}

/// One provider's connection inside one workspace.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct WorkspaceProvider {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Provider identifier, such as `slack` or `github`.
    pub provider: String,
}

/// One login session.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Session {
    /// Login session identifier (version 7 universally unique identifier, or UUID).
    pub session_id: String,
}

/// One invite into an account.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Invite {
    /// Invite identifier (version 7 universally unique identifier, or UUID).
    pub invite_id: String,
}

/// One person in an account.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Member {
    /// Member's user identifier (version 7 universally unique identifier, or UUID).
    pub user_id: String,
}

/// One runner, as an operator addresses it.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Runner {
    /// Runner identifier (version 7 universally unique identifier, or UUID).
    pub runner_id: String,
}

/// One lease a runner holds.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Lease {
    /// Lease identifier (version 7 universally unique identifier, or UUID).
    pub lease_id: String,
}

/// One schedule of the fleet a held lease runs.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct LeaseSchedule {
    /// Lease identifier (version 7 universally unique identifier, or UUID).
    pub lease_id: String,
    /// Schedule identifier (version 7 universally unique identifier, or UUID).
    pub schedule_id: String,
}

/// One hosted schedule.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Schedule {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Fleet identifier (version 7 universally unique identifier, or UUID).
    pub fleet_id: String,
    /// Schedule identifier (version 7 universally unique identifier, or UUID).
    pub schedule_id: String,
}

/// One event in a fleet's history.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Event {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Fleet identifier (version 7 universally unique identifier, or UUID).
    pub fleet_id: String,
    /// Event identifier (version 7 universally unique identifier, or UUID).
    pub event_id: String,
}

/// One tool call of one event.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct ToolCall {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Fleet identifier (version 7 universally unique identifier, or UUID).
    pub fleet_id: String,
    /// Event identifier (version 7 universally unique identifier, or UUID).
    pub event_id: String,
    /// Tool call identifier from the activity stream, in `{fence}:{n}` format. Keep it
    /// unchanged and URL-encode it.
    pub call_id: String,
}

/// One memory a fleet holds.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Memory {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Fleet identifier (version 7 universally unique identifier, or UUID).
    pub fleet_id: String,
    /// Memory entry key.
    pub key: String,
}

/// One integration grant a fleet holds.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Grant {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Fleet identifier (version 7 universally unique identifier, or UUID).
    pub fleet_id: String,
    /// Grant identifier (version 7 universally unique identifier, or UUID).
    pub grant_id: String,
}

/// One secret in a workspace, addressed by the name it was stored under.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Secret {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// The name the secret is stored under.
    pub name: String,
}

/// One preference key in a workspace.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Preference {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// The preference key.
    pub pref_key: String,
}

/// One approval gate.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Gate {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Approval gate identifier (version 7 universally unique identifier, or UUID).
    pub gate_id: String,
}

/// Deciding one approval gate.
///
/// The decision is a path segment rather than a custom verb because `matchit`
/// refuses a literal after a parameter inside one segment.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct GateDecision {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Approval gate identifier (version 7 universally unique identifier, or UUID).
    pub gate_id: String,
    /// What to do with the gate: `approve` or `deny`.
    pub decision: String,
}

/// One creator-private draft inside one workspace.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Draft {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Draft identifier (version 7 universally unique identifier, or UUID).
    pub draft_id: String,
}

/// One Library entry a workspace can see.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Library {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Library identifier: a platform slug or your Library entry's version 7 universally
    /// unique identifier (UUID).
    pub library_id: String,
}

/// One immutable revision of a Library entry.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct LibraryRevision {
    /// Workspace identifier (version 7 universally unique identifier, or UUID).
    pub workspace_id: String,
    /// Library identifier: a platform slug or your Library entry's version 7 universally
    /// unique identifier (UUID).
    pub library_id: String,
    /// Revision identifier (version 7 universally unique identifier, or UUID).
    pub revision_id: String,
}

/// One platform Library entry, addressed by its stable slug.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct PlatformLibrary {
    /// Platform Library slug, 1 to 128 characters.
    #[param(min_length = 1, max_length = 128)]
    pub id: String,
}
