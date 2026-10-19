//! Recovery actions come from this bounded vocabulary, never from model output.

use serde::Serialize;

/// Kind of live setup observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    /// Instructions and native setup are ready to save.
    Definition,
    /// The owner's selected runtime model resolves.
    Model,
    /// Wallet covers outstanding reservations and another runtime interval.
    Wallet,
    /// A declared credential is held and readable.
    Credential,
    /// A required connector remains installed.
    Connection,
    /// A real human-approved grant covers the required integration.
    Grant,
    /// Policy interpretation and target setup remain supported.
    Policy,
}

/// Result of a current readiness observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    /// The observed prerequisite exists.
    Ready,
    /// The prerequisite has not been configured or approved.
    Missing,
    /// An existing selection or authority prevents progress.
    Blocked,
}

/// An authorized destination for a human to repair a missing prerequisite.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Open the workspace owner's account model settings.
    AccountModels,
    /// Open billing for the workspace owner.
    Billing,
    /// Open this workspace's connection settings.
    WorkspaceConnections,
    /// Open this workspace's approval inbox.
    Approvals,
    /// Inspect and revise this setup.
    Review,
}

/// Server-owned label and relative application destination.
#[derive(Debug, Clone, Serialize)]
pub struct Action {
    /// Bounded action vocabulary.
    pub kind: ActionKind,
    /// Safe presentation label.
    pub label: String,
    /// Relative application URL derived from authorized identities.
    pub href: String,
}

/// One current observation, distinct from immutable requirements.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// What prerequisite was checked.
    pub kind: CheckKind,
    /// A declared name or bounded setup subject.
    pub subject: String,
    /// Observed readiness.
    pub state: CheckState,
    /// Observation time, excluded from review content identity.
    #[serde(with = "afd_wire::instant")]
    pub observed_at: jiff::Timestamp,
    /// Present only when an authorized human action can help.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<Action>,
}
