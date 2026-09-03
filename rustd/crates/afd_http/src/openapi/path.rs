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
    /// `UUIDv7` of the resource.
    pub id: String,
}

/// Anything under one workspace.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Workspace {
    /// `UUIDv7` of the workspace.
    pub workspace_id: String,
}

/// Anything under one fleet inside one workspace.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Fleet {
    /// `UUIDv7` of the workspace.
    pub workspace_id: String,
    /// `UUIDv7` of the fleet.
    pub fleet_id: String,
}

/// A fleet addressed without its workspace — the signed-delivery surface.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct FleetOnly {
    /// `UUIDv7` of the fleet.
    pub fleet_id: String,
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
    /// `UUIDv7` of the workspace.
    pub workspace_id: String,
    /// Provider identifier, such as `slack` or `github`.
    pub provider: String,
}

/// One login session.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Session {
    /// `UUIDv7` of the login session.
    pub session_id: String,
}

/// One runner, as an operator addresses it.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Runner {
    /// `UUIDv7` of the runner.
    pub runner_id: String,
}

/// One lease a runner holds.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Lease {
    /// `UUIDv7` of the lease.
    pub lease_id: String,
}

/// One hosted schedule.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Schedule {
    /// `UUIDv7` of the workspace.
    pub workspace_id: String,
    /// `UUIDv7` of the fleet.
    pub fleet_id: String,
    /// `UUIDv7` of the schedule.
    pub schedule_id: String,
}

/// One event in a fleet's history.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Event {
    /// `UUIDv7` of the workspace.
    pub workspace_id: String,
    /// `UUIDv7` of the fleet.
    pub fleet_id: String,
    /// `UUIDv7` of the event.
    pub event_id: String,
}

/// One memory a fleet holds.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Memory {
    /// `UUIDv7` of the workspace.
    pub workspace_id: String,
    /// `UUIDv7` of the fleet.
    pub fleet_id: String,
    /// Memory entry key.
    pub key: String,
}

/// One integration grant a fleet holds.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Grant {
    /// `UUIDv7` of the workspace.
    pub workspace_id: String,
    /// `UUIDv7` of the fleet.
    pub fleet_id: String,
    /// `UUIDv7` of the grant.
    pub grant_id: String,
}

/// One secret in a workspace, addressed by the name it was stored under.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Secret {
    /// `UUIDv7` of the workspace.
    pub workspace_id: String,
    /// The name the secret is stored under.
    pub name: String,
}

/// One preference key in a workspace.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Preference {
    /// `UUIDv7` of the workspace.
    pub workspace_id: String,
    /// The preference key.
    pub pref_key: String,
}

/// One approval gate.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Gate {
    /// `UUIDv7` of the workspace.
    pub workspace_id: String,
    /// `UUIDv7` of the approval gate.
    pub gate_id: String,
}

/// Deciding one approval gate.
///
/// The decision is a path segment rather than a custom verb because `matchit`
/// refuses a literal after a parameter inside one segment.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct GateDecision {
    /// `UUIDv7` of the workspace.
    pub workspace_id: String,
    /// `UUIDv7` of the approval gate.
    pub gate_id: String,
    /// What to do with the gate: `approve` or `deny`.
    pub decision: String,
}

/// One bundle, addressed by the hash of its content.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct Bundle {
    /// Content hash of the bundle.
    pub content_hash: String,
}
