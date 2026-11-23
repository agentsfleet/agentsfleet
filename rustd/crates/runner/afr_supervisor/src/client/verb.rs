//! Every runner verb: its name, its method, and the code its failure is
//! logged under.
//!
//! Split from [`super`], which sends them, so the vocabulary reads in one
//! place: a verb added here without a method, a name or a code does not
//! compile.

use std::fmt;

use af_core::error_code::{self, ErrorCode};

/// The HTTP method a verb is sent with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Method {
    /// A read.
    Get,
    /// A conditional full replacement.
    Put,
    /// A report, a create, or an operation.
    Post,
    /// A partial change.
    Patch,
    /// A removal.
    Delete,
}

impl Method {
    /// The method as the HTTP client spells it.
    pub(crate) const fn http(self) -> reqwest::Method {
        match self {
            Self::Get => reqwest::Method::GET,
            Self::Put => reqwest::Method::PUT,
            Self::Post => reqwest::Method::POST,
            Self::Patch => reqwest::Method::PATCH,
            Self::Delete => reqwest::Method::DELETE,
        }
    }
}

/// One runner verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Verb {
    /// The authenticated opening of the native runner stream.
    Open,
    /// Liveness, capability, assignment.
    Heartbeat,
    /// More time on a held run.
    Renew,
    /// Live-tail frames for a held run.
    Progress,
    /// A run's terminal result.
    Report,
    /// A fleet's memory at run start.
    Load,
    /// A fleet's memory written back.
    Capture,
    /// A search of a fleet's memory past the window.
    Recall,
    /// A scoped credential for a held run.
    Mint,
    /// Finished calls' full records for a held run.
    Items,
    /// The runner's own row: which runner this is, as the daemon names it.
    Me,
    /// A schedule created for the held fleet.
    ScheduleCreate,
    /// The held fleet's schedules.
    ScheduleList,
    /// A change to a schedule the fleet made.
    ScheduleUpdate,
    /// A schedule the fleet made, deleted.
    ScheduleDelete,
    /// A run of a schedule, created now.
    ScheduleRun,
    /// A schedule's runs.
    ScheduleRuns,
    /// A line said to the held turn's thread.
    Message,
    /// The held fleet's workspace's fleets.
    FleetList,
    /// One fleet's recent turns, as the held fleet reads them.
    FleetTurns,
    /// Work the held fleet puts on another fleet.
    FleetSteer,
    /// The messages a person sent while the run works, assigned to it.
    Folds,
    /// Native event-owned recovery command.
    TurnCheckpoint,
    /// A setup run's read of its draft's compacted context.
    ContextLoad,
    /// A setup run's fenced save of its draft's compacted context.
    ContextSave,
    /// Complete native human input and recovered meaning.
    DurableTaskInputs,
    /// One complete immutable human interpretation.
    DurableTaskBindings,
    /// Accepted input application or original reply.
    DurableTaskInputReceipt,
    /// One native physical decision reservation.
    DurableTaskDecisionReserve,
    /// Original-owner physical decision accounting.
    DurableTaskDecisionRecord,
    /// One exact-head native publication.
    DurableTaskEffectPublish,
    /// Recovery effects beyond a confirmed watermark.
    DurableTaskEffects,
    /// One retained publication receipt.
    DurableTaskEffect,
    /// An immutable capture plan and writer pin.
    DurableTaskCheckpointPlan,
    /// A complete verified manifest publication.
    DurableTaskCheckpointCommit,
    /// An immutable manifest page.
    DurableTaskCheckpointPageWrite,
    /// One authorized recovery page.
    DurableTaskCheckpointPage,
    /// Bounded native plaintext object registration.
    DurableTaskCheckpointObjectsRegister,
    /// Immutable ciphertext and create-only transfers.
    DurableTaskCheckpointUploads,
    /// Native restore custody and transfer refresh.
    DurableTaskCheckpointAuthorizations,
    /// Versioned native reader or writer pin renewal.
    DurableTaskCheckpointPin,
}

impl Verb {
    /// Every verb, so a suite walks the whole vocabulary rather than a
    /// hand-kept list that a new verb can miss.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 41] = [
        Self::Open,
        Self::Heartbeat,
        Self::Renew,
        Self::Progress,
        Self::Report,
        Self::Load,
        Self::Capture,
        Self::Recall,
        Self::Mint,
        Self::Items,
        Self::Me,
        Self::ScheduleCreate,
        Self::ScheduleList,
        Self::ScheduleUpdate,
        Self::ScheduleDelete,
        Self::ScheduleRun,
        Self::ScheduleRuns,
        Self::Message,
        Self::FleetList,
        Self::FleetTurns,
        Self::FleetSteer,
        Self::Folds,
        Self::TurnCheckpoint,
        Self::ContextLoad,
        Self::ContextSave,
        Self::DurableTaskInputs,
        Self::DurableTaskBindings,
        Self::DurableTaskInputReceipt,
        Self::DurableTaskDecisionReserve,
        Self::DurableTaskDecisionRecord,
        Self::DurableTaskEffectPublish,
        Self::DurableTaskEffects,
        Self::DurableTaskEffect,
        Self::DurableTaskCheckpointPlan,
        Self::DurableTaskCheckpointCommit,
        Self::DurableTaskCheckpointPageWrite,
        Self::DurableTaskCheckpointPage,
        Self::DurableTaskCheckpointObjectsRegister,
        Self::DurableTaskCheckpointUploads,
        Self::DurableTaskCheckpointAuthorizations,
        Self::DurableTaskCheckpointPin,
    ];

    /// The method this verb is sent with.
    pub(crate) const fn method(self) -> Method {
        match self {
            Self::Load
            | Self::Me
            | Self::ScheduleList
            | Self::ScheduleRuns
            | Self::FleetList
            | Self::FleetTurns
            | Self::DurableTaskInputs
            | Self::DurableTaskEffects
            | Self::DurableTaskEffect
            | Self::DurableTaskCheckpointPage => Method::Get,
            Self::DurableTaskCheckpointPageWrite => Method::Put,
            Self::ScheduleUpdate
            | Self::DurableTaskDecisionRecord
            | Self::DurableTaskCheckpointCommit
            | Self::DurableTaskCheckpointPin => Method::Patch,
            Self::ScheduleDelete => Method::Delete,
            Self::Open
            | Self::Heartbeat
            | Self::Renew
            | Self::Progress
            | Self::Report
            | Self::Capture
            | Self::Recall
            | Self::Mint
            | Self::Items
            | Self::ScheduleCreate
            | Self::ScheduleRun
            | Self::Message
            | Self::FleetSteer
            | Self::Folds
            | Self::TurnCheckpoint
            | Self::ContextLoad
            | Self::ContextSave
            | Self::DurableTaskBindings
            | Self::DurableTaskInputReceipt
            | Self::DurableTaskDecisionReserve
            | Self::DurableTaskEffectPublish
            | Self::DurableTaskCheckpointPlan
            | Self::DurableTaskCheckpointObjectsRegister
            | Self::DurableTaskCheckpointUploads
            | Self::DurableTaskCheckpointAuthorizations => Method::Post,
        }
    }

    /// The verb as a log line and an error name it.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "stream_open",
            Self::Heartbeat => "heartbeat",
            Self::Renew => "renew",
            Self::Progress => "progress",
            Self::Report => "report",
            Self::Load => "load",
            Self::Capture => "capture",
            Self::Recall => "recall",
            Self::Mint => "mint",
            Self::Items => "items",
            Self::Me => "me",
            Self::ScheduleCreate => "schedule_create",
            Self::ScheduleList => "schedule_list",
            Self::ScheduleUpdate => "schedule_update",
            Self::ScheduleDelete => "schedule_delete",
            Self::ScheduleRun => "schedule_run",
            Self::ScheduleRuns => "schedule_runs",
            Self::Message => "message",
            Self::FleetList => "fleet_list",
            Self::FleetTurns => "fleet_turns",
            Self::FleetSteer => "fleet_steer",
            Self::Folds => "folds",
            Self::TurnCheckpoint => "turn_checkpoint",
            Self::ContextLoad => "context_load",
            Self::ContextSave => "context_save",
            Self::DurableTaskInputs => "durable_task_inputs",
            Self::DurableTaskBindings => "durable_task_bindings",
            Self::DurableTaskInputReceipt => "task_input_receipt",
            Self::DurableTaskDecisionReserve => "task_decision_reserve",
            Self::DurableTaskDecisionRecord => "task_decision_record",
            Self::DurableTaskEffectPublish => "task_effect_publish",
            Self::DurableTaskEffects => "durable_task_effects",
            Self::DurableTaskEffect => "durable_task_effect",
            Self::DurableTaskCheckpointPlan => "task_checkpoint_plan",
            Self::DurableTaskCheckpointCommit => "task_checkpoint_commit",
            Self::DurableTaskCheckpointPageWrite => "durable_task_checkpoint_page_write",
            Self::DurableTaskCheckpointPage => "task_checkpoint_page",
            Self::DurableTaskCheckpointObjectsRegister => {
                "durable_task_checkpoint_objects_register"
            }
            Self::DurableTaskCheckpointUploads => "durable_task_checkpoint_uploads",
            Self::DurableTaskCheckpointAuthorizations => "durable_task_checkpoint_authorizations",
            Self::DurableTaskCheckpointPin => "task_checkpoint_pin",
        }
    }

    /// The registry code a failure of this verb is logged under.
    pub(crate) const fn code(self) -> ErrorCode {
        match self {
            Self::Load | Self::Capture | Self::Recall => error_code::MEM_UNAVAILABLE,
            Self::Open
            | Self::Heartbeat
            | Self::Renew
            | Self::Progress
            | Self::Report
            | Self::Mint
            | Self::Items
            | Self::Me
            | Self::ScheduleCreate
            | Self::ScheduleList
            | Self::ScheduleUpdate
            | Self::ScheduleDelete
            | Self::ScheduleRun
            | Self::ScheduleRuns
            | Self::Message
            | Self::FleetList
            | Self::FleetTurns
            | Self::FleetSteer
            | Self::Folds
            | Self::TurnCheckpoint
            | Self::ContextLoad
            | Self::ContextSave
            | Self::DurableTaskInputs
            | Self::DurableTaskBindings
            | Self::DurableTaskInputReceipt
            | Self::DurableTaskDecisionReserve
            | Self::DurableTaskDecisionRecord
            | Self::DurableTaskEffectPublish
            | Self::DurableTaskEffects
            | Self::DurableTaskEffect
            | Self::DurableTaskCheckpointPlan
            | Self::DurableTaskCheckpointCommit
            | Self::DurableTaskCheckpointPageWrite
            | Self::DurableTaskCheckpointPage
            | Self::DurableTaskCheckpointObjectsRegister
            | Self::DurableTaskCheckpointUploads
            | Self::DurableTaskCheckpointAuthorizations
            | Self::DurableTaskCheckpointPin => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

impl fmt::Display for Verb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
