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
    /// Native runner control, work, report and activity lanes.
    Stream,
    /// Reserve a physical decision with accepted native questions and credit.
    DurableTaskDecisions,
    /// Read complete pending human messages without consuming them.
    DurableTaskInputs,
    /// Accept complete message interpretation and native objective associations.
    DurableTaskBindings,
    /// Native exact-head publication and recovery pages.
    DurableTaskEffects,
    /// Read and reconcile one immutable publication.
    DurableTaskEffect,
    /// Source-bound input application and reply receipts.
    DurableTaskInputReceipts,
    /// Native immutable capture plans.
    DurableTaskCheckpoints,
    /// Complete verified manifest publication.
    DurableTaskCheckpoint,
    /// Immutable bounded manifest pages.
    DurableTaskCheckpointPage,
    /// Object registration and bounded restore reads.
    DurableTaskCheckpointObjects,
    /// Frozen ciphertext descriptors and conditional PUT authorization.
    DurableTaskCheckpointUploads,
    /// Restore bootstrap and bounded transfer refresh.
    DurableTaskCheckpointAuthorizations,
    /// Native capture/restore pin renewal.
    DurableTaskCheckpointPin,
    /// Record one original-owner decision result, including late usage.
    DurableTaskDecision,
    /// Minting the per-run credentials a fleet needs.
    CredentialsMint,
    /// Loading a fleet's memory at run start.
    MemoryLoad,
    /// Writing a fleet's memory back.
    MemoryCapture,
    /// Searching a fleet's memory past the window a run was seeded with.
    MemoryRecall,
    /// Keeping each finished tool call's full arguments and output.
    Items,
    /// The schedules of the fleet a held run runs: listing them, and a
    /// fleet creating its own.
    Schedules,
    /// One schedule the fleet created: changing it, and deleting it.
    Schedule,
    /// One schedule's runs: reading them, and creating one to fire it now.
    ScheduleRuns,
    /// What may wake the held fleet besides its schedules: listing its
    /// triggers, and the fleet adding one.
    Triggers,
    /// One trigger of the held fleet: removing it.
    Trigger,
    /// The held fleet asking a person for a secret or an integration.
    Access,
    /// A line said to the event's thread before the answer.
    Messages,
    /// Work the held fleet puts on another fleet in its workspace.
    Steers,
    /// The fleets of the held fleet's workspace.
    Fleets,
    /// One fleet's recent events, as the held fleet reads them.
    FleetTurns,
}

impl RunnerRoute {
    /// Every runner-plane route.
    pub const ALL: &'static [Self] = &[
        Self::SelfRecord,
        Self::Stream,
        Self::DurableTaskDecisions,
        Self::DurableTaskInputs,
        Self::DurableTaskBindings,
        Self::DurableTaskEffects,
        Self::DurableTaskEffect,
        Self::DurableTaskInputReceipts,
        Self::DurableTaskCheckpoints,
        Self::DurableTaskCheckpoint,
        Self::DurableTaskCheckpointPage,
        Self::DurableTaskCheckpointObjects,
        Self::DurableTaskCheckpointUploads,
        Self::DurableTaskCheckpointAuthorizations,
        Self::DurableTaskCheckpointPin,
        Self::DurableTaskDecision,
        Self::CredentialsMint,
        Self::MemoryLoad,
        Self::MemoryCapture,
        Self::MemoryRecall,
        Self::Items,
        Self::Schedules,
        Self::Schedule,
        Self::ScheduleRuns,
        Self::Triggers,
        Self::Trigger,
        Self::Access,
        Self::Messages,
        Self::Steers,
        Self::Fleets,
        Self::FleetTurns,
    ];

    /// The verbs this route identity serves.
    ///
    /// Reads are the runner asking what it has been given — its own record, a
    /// fleet's memory. Reporting is a `POST` because
    /// each one appends a fact rather than replacing a resource. The schedules
    /// and triggers are the resources a runner manages: collections it lists
    /// and adds to, and members it removes; a schedule is also edited, and its
    /// runs read and created.
    #[must_use]
    pub const fn verbs(self) -> &'static [Verb] {
        match self {
            Self::DurableTaskCheckpointPage => &[Verb::Get, Verb::Put],
            Self::Schedules
            | Self::ScheduleRuns
            | Self::Triggers
            | Self::DurableTaskEffects
            | Self::DurableTaskCheckpointObjects => &[Verb::Get, Verb::Post],
            Self::Schedule => &[Verb::Patch, Verb::Delete],
            Self::Trigger => &[Verb::Delete],
            Self::DurableTaskDecision
            | Self::DurableTaskCheckpoint
            | Self::DurableTaskCheckpointPin => &[Verb::Patch],
            Self::SelfRecord
            | Self::MemoryLoad
            | Self::DurableTaskInputs
            | Self::DurableTaskEffect
            | Self::Fleets
            | Self::FleetTurns => &[Verb::Get],
            Self::Stream
            | Self::CredentialsMint
            | Self::MemoryCapture
            | Self::MemoryRecall
            | Self::Items
            | Self::DurableTaskDecisions
            | Self::DurableTaskBindings
            | Self::DurableTaskInputReceipts
            | Self::DurableTaskCheckpoints
            | Self::DurableTaskCheckpointUploads
            | Self::DurableTaskCheckpointAuthorizations
            | Self::Messages
            | Self::Access
            | Self::Steers => &[Verb::Post],
        }
    }

    /// Load and capture share a path and differ by method, so they share an
    /// arm here. They stay two routes because they are two operations — one
    /// reads a fleet's memory at run start, the other writes it back — and
    /// collapsing them would lose the distinction every other table keys on.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let template = match self {
            Self::SelfRecord => paths::RUNNER_SELF,
            Self::Stream => paths::RUNNER_STREAM,
            Self::DurableTaskDecisions => paths::RUN_DURABLE_TASK_DECISIONS,
            Self::DurableTaskInputs => paths::RUN_DURABLE_TASK_INPUTS,
            Self::DurableTaskBindings => paths::RUN_DURABLE_TASK_BINDINGS,
            Self::DurableTaskEffects => paths::RUN_DURABLE_TASK_EFFECTS,
            Self::DurableTaskEffect => paths::RUN_DURABLE_TASK_EFFECT,
            Self::DurableTaskInputReceipts => paths::RUN_DURABLE_TASK_INPUT_RECEIPTS,
            Self::DurableTaskCheckpoints => paths::RUN_DURABLE_TASK_CHECKPOINTS,
            Self::DurableTaskCheckpoint => paths::RUN_DURABLE_TASK_CHECKPOINT,
            Self::DurableTaskCheckpointPage => paths::RUN_DURABLE_TASK_CHECKPOINT_PAGE,
            Self::DurableTaskCheckpointObjects => paths::RUN_DURABLE_TASK_CHECKPOINT_OBJECTS,
            Self::DurableTaskCheckpointUploads => paths::RUN_DURABLE_TASK_CHECKPOINT_UPLOADS,
            Self::DurableTaskCheckpointAuthorizations => {
                paths::RUN_DURABLE_TASK_CHECKPOINT_AUTHORIZATIONS
            }
            Self::DurableTaskCheckpointPin => paths::RUN_DURABLE_TASK_CHECKPOINT_PIN,
            Self::DurableTaskDecision => paths::RUN_DURABLE_TASK_DECISION,
            Self::CredentialsMint => paths::RUNNER_CREDENTIALS_MINT,
            Self::MemoryLoad | Self::MemoryCapture => paths::RUNNER_MEMORY_FLEET,
            Self::MemoryRecall => paths::RUNNER_MEMORY_RECALL,
            Self::Items => paths::RUN_ITEMS,
            Self::Schedules => paths::RUN_SCHEDULES,
            Self::Schedule => paths::RUN_SCHEDULE,
            Self::ScheduleRuns => paths::RUN_SCHEDULE_RUNS,
            Self::Triggers => paths::RUN_TRIGGERS,
            Self::Trigger => paths::RUN_TRIGGER,
            Self::Access => paths::RUN_ACCESS,
            Self::Messages => paths::RUN_MESSAGES,
            Self::Steers => paths::RUN_STEERS,
            Self::Fleets => paths::RUN_FLEETS,
            Self::FleetTurns => paths::RUN_FLEET_TURNS,
        };
        RouteMeta::new(
            Guard::RunnerBearer,
            match self {
                Self::Stream => RouteClass::Stream,
                _ => RouteClass::Api,
            },
            template,
            Scopes::Always(RUNNER_SELF),
        )
    }
}
