//! Only an already-verified ingress adapter supplies completion identity.

use super::super::TaskScope;
use afd_core::id::Uuid7;

/// App events belong to an installation workspace; fleet hook events remain fleet-scoped.
#[derive(Debug, Clone)]
pub enum CompletionScope {
    /// Verified installation event may signal matching waits in this workspace.
    Workspace(Uuid7),
    /// Verified fleet hook may signal matching waits only in this fleet scope.
    Fleet(TaskScope),
}

/// Native completion identity; it contains no URL, prompt or credential.
#[derive(Debug, Clone, garde::Validate)]
pub struct VerifiedCompletion {
    /// Native authorization proved before payload interpretation.
    #[garde(skip)]
    pub scope: CompletionScope,
    /// Connected installation identity.
    #[garde(range(min = 1, max = i64::MAX as u64))]
    pub installation_id: u64,
    /// Repository identity from the signed body.
    #[garde(range(min = 1, max = i64::MAX as u64))]
    pub repository_id: u64,
    /// Exact run head.
    #[garde(length(bytes, min = 40, max = 40), pattern(r"^[0-9a-f]{40}$"))]
    pub head: String,
    /// Native reviewed workflow identity.
    #[garde(range(min = 1, max = i64::MAX as u64))]
    pub workflow_id: u64,
    /// Run identity; attempts never share a completion receipt.
    #[garde(range(min = 1, max = i64::MAX as u64))]
    pub run_id: u64,
    /// Monotonic number within the reviewed workflow.
    #[garde(range(min = 1, max = i64::MAX as u64))]
    pub run_number: u64,
    /// GitHub's actual run attempt.
    #[garde(range(min = 1, max = i32::MAX as u32))]
    pub run_attempt: u32,
    /// Bounded provider conclusion; it never completes a task directly.
    #[garde(length(bytes, min = 1, max = 64), pattern(r"^[a-z_]+$"))]
    pub conclusion: String,
    /// Digest of the body whose signature was verified.
    #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]{64}$"))]
    pub body_digest: String,
}

/// Bounded maintenance result; identities stay in traces, never metric labels.
#[derive(Debug, Default)]
pub struct WaitMaintenance {
    /// Native status observations attempted.
    pub checked: u32,
    /// One current wait consumed through ordinary admission.
    pub woken: u32,
    /// Reads deferred or refused without an agent or model call.
    pub deferred: u32,
}
