//! The parameters of the calls on the guest's environment rather than a
//! process or a file: the recovery proof, renewal, the hosts file, the tenant
//! leaf's counts, and its freeze and thaw.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// The proof requested before freeze excludes process and provider state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryParams {
    /// Which work the proof inspects.
    pub proof: RecoveryProof,
}

/// `env/renew` parameters: what becomes of the workspace.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenewParams {
    /// What the renewal leaves of the workspace.
    pub workspace: RenewedWorkspace,
}

/// `env/hosts` parameters: the hosts file, whole. The frame is its only
/// bound.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostsParams<'a> {
    /// The hosts file's whole text.
    pub file: Cow<'a, str>,
}

/// `env/count` parameters: which count.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountParams {
    /// Which count.
    pub count: Counted,
}

/// The counts the tenant leaf keeps that the host reads.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Counted {
    /// Forks refused at the leaf's process cap since the guest booted.
    PidsRefused,
}

/// `env/freeze` and `env/thaw` parameters: which leaf.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeafParams {
    /// Which leaf.
    pub leaf: Leaf,
}

/// The leaves a freeze or thaw names: the tenant's, the one leaf the guest agent
/// was given.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Leaf {
    /// The leaf every tenant process is placed in.
    Tenant,
}

/// What a renewal leaves of the workspace: nothing, for a restore to import
/// its tree into.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenewedWorkspace {
    /// Emptied, for a restore to import its tree into.
    Emptied,
}

/// Each supported proof names the agent-owned work it actually inspects.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryProof {
    /// No call runs and no process placed in the tenant leaf remains.
    FileJobs,
    /// No call runs; a tenant process left behind is not counted, since the
    /// pin pause freezes it.
    Calls,
}
