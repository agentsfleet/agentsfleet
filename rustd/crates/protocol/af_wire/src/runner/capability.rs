//! Bounded host observations, including the build that produced each heartbeat.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::{Isolation, Kvm, SandboxEngine};

/// What this host can actually enforce.
//
// Probed at startup and refreshed per beat. The engine fields say what builds
// the sandboxes and the wall it puts around them, so the daemon reads the
// engine as proof of its tier and never asks for a host mechanism.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CapabilityReport<'a> {
    /// This build supports fenced task work and workspace handoff.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[garde(skip)]
    pub durable_tasks_v1: bool,
    /// Source digest compiled into the reporting runner; absent for an unbound build.
    #[serde(borrow, default)]
    #[garde(inner(length(bytes, min = 64, max = 64)))]
    pub build_id: Option<Cow<'a, str>>,
    /// The engine that builds this runner's sandboxes.
    #[garde(skip)]
    pub engine: SandboxEngine,
    /// The wall that engine puts between a sandbox's code and the host.
    #[garde(skip)]
    pub isolation: Isolation,
    /// What the host's virtualization device offers.
    #[garde(skip)]
    pub kvm: Kvm,
    /// How many sandboxes the host's memory holds at once; absent where the
    /// engine reserves no memory per sandbox.
    // At least one; at most the Postgres `INTEGER` a count is stored in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(minimum = 1, maximum = 2_147_483_647_u32))]
    #[garde(inner(range(min = 1, max = i32::MAX.unsigned_abs())))]
    pub sandboxes_max: Option<u32>,
}

/// What the host offers its runs, as one beat read it. A fact the host could
/// not read is absent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct HostFacts {
    /// Processors the runner may use.
    // At least one; at most the Postgres `INTEGER` column that stores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(minimum = 1, maximum = 2_147_483_647_u32))]
    #[garde(inner(range(min = 1, max = i32::MAX.unsigned_abs())))]
    pub cpu_count: Option<u32>,
    /// The host's total memory, in bytes.
    // At most the Postgres `BIGINT` column that stores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(maximum = 9_223_372_036_854_775_807_u64))]
    #[garde(inner(range(max = i64::MAX.unsigned_abs())))]
    pub memory_bytes: Option<u64>,
    /// Free bytes on the file system that holds the runner's sandboxes.
    // At most the Postgres `BIGINT` column that stores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(maximum = 9_223_372_036_854_775_807_u64))]
    #[garde(inner(range(max = i64::MAX.unsigned_abs())))]
    pub free_disk_bytes: Option<u64>,
}
