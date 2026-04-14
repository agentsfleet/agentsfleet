//! Workspace entitlement policy types — tier-scoped billing limits.
//!
//! Consumed by `src/state/workspace_billing/db.zig` to map a plan tier to its
//! resource-limit envelope. The v1 pipeline-profile validation branch
//! (`enforceWithAudit`, `evaluateProfile`, topology-backed stage/skill
//! auditing) was removed in M17_002 alongside the executor alignment work —
//! it had no live callers after the zombie migration.

const std = @import("std");

pub const Boundary = enum {
    compile,
    activate,
    runtime,
};

pub const PolicyTier = enum {
    free,
    scale,
    unknown,
};

pub const EntitlementPolicy = struct {
    tier: PolicyTier,
    max_stages: u16,
    max_distinct_skills: u16,
    allow_custom_skills: bool,
};
