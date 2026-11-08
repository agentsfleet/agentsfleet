//! The runner plane: a host speaking for itself with an `agt_r` token.
//!
//! Every route here is `Guard::RunnerBearer` and every one requires
//! [`afd_auth::Scope::RunnerSelf`]. A tenant credential arriving here is
//! refused before any lookup by [`afd_auth::Plane`] — the boundary is data,
//! not which middleware happened to be mounted, which is what makes it a fact
//! the type system can hold rather than a wiring convention.
//!
//! The operator's view over runners is [`super::runner_ops`].

use af_wire::paths;
use afd_auth::Scope;

use super::{Guard, RouteClass, RouteMeta, Scopes, Verb};

/// What a runner may do on its own behalf. One scope, because the plane IS the
/// authorisation: a runner token is not a capability a person hands out.
const RUNNER_SELF: &[Scope] = &[Scope::RunnerSelf];

/// A runner-plane route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunnerRoute {
    /// The runner reading its own record.
    SelfRecord,
    /// Private context checkpoint for a branch of a live lease.
    ContextCheckpoint,
    /// Reserve a physical decision with accepted native questions and credit.
    TaskDecisions,
    /// Read complete pending human messages without consuming them.
    TaskInputs,
    /// Accept complete message interpretation and native objective associations.
    TaskBindings,
    /// Native exact-head publication and recovery pages.
    TaskEffects,
    /// Read and reconcile one immutable publication.
    TaskEffect,
    /// Source-bound input application and reply receipts.
    TaskInputReceipts,
    /// Native immutable capture plans.
    TaskCheckpoints,
    /// Complete verified manifest publication.
    TaskCheckpoint,
    /// Immutable bounded manifest pages.
    TaskCheckpointPage,
    /// Object registration and bounded restore reads.
    TaskCheckpointObjects,
    /// Frozen ciphertext descriptors and conditional PUT authorization.
    TaskCheckpointUploads,
    /// Restore bootstrap and bounded transfer refresh.
    TaskCheckpointAuthorizations,
    /// Native capture/restore pin renewal.
    TaskCheckpointPin,
    /// Record one original-owner decision result, including late usage.
    TaskDecision,
    /// The runner's heartbeat.
    Heartbeat,
    /// Claiming a lease.
    Lease,
    /// Reporting on a lease.
    Report,
    /// Minting the per-lease credentials a fleet needs.
    CredentialsMint,
    /// Reporting activity against a held lease.
    Activity,
    /// Version-bound private setup proposals and progress.
    AuthoringActivity,
    /// Scoped daemon provider invocation without a raw provider key.
    AuthoringModelCalls,
    /// Renewing private authoring with the shared host capacity guard.
    AuthoringRenew,
    /// Terminal answer or content-free accounting recovery for private work.
    AuthoringReports,
    /// Renewing a held lease.
    Renew,
    /// Loading a fleet's memory at lease start.
    MemoryHydrate,
    /// Writing a fleet's memory back.
    MemoryCapture,
    /// Searching a fleet's memory past the window a run was seeded with.
    MemoryRecall,
    /// Keeping each finished tool call's full arguments and output.
    ToolCalls,
    /// The schedules of the fleet a held lease runs: listing them, and a
    /// fleet creating its own.
    Schedules,
    /// One schedule the fleet created: changing it, and deleting it.
    Schedule,
    /// One schedule's runs: reading them, and creating one to fire it now.
    ScheduleRuns,
    /// A line said to the event's thread before the answer.
    Messages,
    /// The messages the fleet received while the run works, claimed for it.
    Folds,
}

impl RunnerRoute {
    /// Every runner-plane route.
    pub const ALL: &'static [Self] = &[
        Self::SelfRecord,
        Self::ContextCheckpoint,
        Self::TaskDecisions,
        Self::TaskInputs,
        Self::TaskBindings,
        Self::TaskEffects,
        Self::TaskEffect,
        Self::TaskInputReceipts,
        Self::TaskCheckpoints,
        Self::TaskCheckpoint,
        Self::TaskCheckpointPage,
        Self::TaskCheckpointObjects,
        Self::TaskCheckpointUploads,
        Self::TaskCheckpointAuthorizations,
        Self::TaskCheckpointPin,
        Self::TaskDecision,
        Self::Heartbeat,
        Self::Lease,
        Self::Report,
        Self::CredentialsMint,
        Self::Activity,
        Self::AuthoringActivity,
        Self::AuthoringModelCalls,
        Self::AuthoringRenew,
        Self::AuthoringReports,
        Self::Renew,
        Self::MemoryHydrate,
        Self::MemoryCapture,
        Self::MemoryRecall,
        Self::ToolCalls,
        Self::Schedules,
        Self::Schedule,
        Self::ScheduleRuns,
        Self::Messages,
        Self::Folds,
    ];

    /// The verbs this route identity serves.
    ///
    /// Reads are the runner asking what it has been given — its own record, a
    /// fleet's memory. Reporting is a `POST` because
    /// each one appends a fact rather than replacing a resource. The schedules
    /// are the one resource a runner manages: a collection it lists and adds
    /// to, members it edits and removes, and runs it reads and creates.
    #[must_use]
    pub const fn verbs(self) -> &'static [Verb] {
        match self {
            Self::ContextCheckpoint | Self::TaskCheckpointPage => &[Verb::Get, Verb::Put],
            Self::Schedules
            | Self::ScheduleRuns
            | Self::TaskEffects
            | Self::TaskCheckpointObjects => &[Verb::Get, Verb::Post],
            Self::Schedule => &[Verb::Patch, Verb::Delete],
            Self::TaskDecision | Self::TaskCheckpoint | Self::TaskCheckpointPin => &[Verb::Patch],
            Self::SelfRecord | Self::MemoryHydrate | Self::TaskInputs | Self::TaskEffect => {
                &[Verb::Get]
            }
            Self::Heartbeat
            | Self::Lease
            | Self::Report
            | Self::CredentialsMint
            | Self::Activity
            | Self::AuthoringActivity
            | Self::AuthoringModelCalls
            | Self::AuthoringRenew
            | Self::AuthoringReports
            | Self::Renew
            | Self::MemoryCapture
            | Self::MemoryRecall
            | Self::ToolCalls
            | Self::TaskDecisions
            | Self::TaskBindings
            | Self::TaskInputReceipts
            | Self::TaskCheckpoints
            | Self::TaskCheckpointUploads
            | Self::TaskCheckpointAuthorizations
            | Self::Messages
            | Self::Folds => &[Verb::Post],
        }
    }

    /// Hydrate and capture share a path and differ by method, so they share an
    /// arm here. They stay two routes because they are two operations — one
    /// reads a fleet's memory at lease start, the other writes it back — and
    /// collapsing them would lose the distinction every other table keys on.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let template = match self {
            Self::SelfRecord => paths::RUNNER_SELF,
            Self::ContextCheckpoint => paths::LEASE_CONTEXT,
            Self::TaskDecisions => paths::LEASE_TASK_DECISIONS,
            Self::TaskInputs => paths::LEASE_TASK_INPUTS,
            Self::TaskBindings => paths::LEASE_TASK_BINDINGS,
            Self::TaskEffects => paths::LEASE_TASK_EFFECTS,
            Self::TaskEffect => paths::LEASE_TASK_EFFECT,
            Self::TaskInputReceipts => paths::LEASE_TASK_INPUT_RECEIPTS,
            Self::TaskCheckpoints => paths::LEASE_TASK_CHECKPOINTS,
            Self::TaskCheckpoint => paths::LEASE_TASK_CHECKPOINT,
            Self::TaskCheckpointPage => paths::LEASE_TASK_CHECKPOINT_PAGE,
            Self::TaskCheckpointObjects => paths::LEASE_TASK_CHECKPOINT_OBJECTS,
            Self::TaskCheckpointUploads => paths::LEASE_TASK_CHECKPOINT_UPLOADS,
            Self::TaskCheckpointAuthorizations => paths::LEASE_TASK_CHECKPOINT_AUTHORIZATIONS,
            Self::TaskCheckpointPin => paths::LEASE_TASK_CHECKPOINT_PIN,
            Self::TaskDecision => paths::LEASE_TASK_DECISION,
            Self::Heartbeat => paths::RUNNER_HEARTBEATS,
            Self::Lease => paths::RUNNER_LEASES,
            Self::Report => paths::RUNNER_REPORTS,
            Self::CredentialsMint => paths::RUNNER_CREDENTIALS_MINT,
            Self::Activity => paths::LEASE_ACTIVITY,
            Self::AuthoringActivity => paths::AUTHORING_ACTIVITY,
            Self::AuthoringModelCalls => paths::AUTHORING_MODEL_CALLS,
            Self::AuthoringRenew => paths::AUTHORING_RENEW,
            Self::AuthoringReports => paths::AUTHORING_REPORTS,
            Self::Renew => paths::LEASE_RENEW,
            Self::MemoryHydrate | Self::MemoryCapture => paths::RUNNER_MEMORY_FLEET,
            Self::MemoryRecall => paths::RUNNER_MEMORY_RECALL,
            Self::ToolCalls => paths::LEASE_TOOL_CALLS,
            Self::Schedules => paths::LEASE_SCHEDULES,
            Self::Schedule => paths::LEASE_SCHEDULE,
            Self::ScheduleRuns => paths::LEASE_SCHEDULE_RUNS,
            Self::Messages => paths::LEASE_MESSAGES,
            Self::Folds => paths::LEASE_FOLDS,
        };
        RouteMeta::new(
            Guard::RunnerBearer,
            RouteClass::Api,
            template,
            Scopes::Always(RUNNER_SELF),
        )
    }
}
