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
    /// Liveness, capability, assignment.
    Heartbeat,
    /// Private run working notes, loaded under a live fence.
    ContextLoad,
    /// Private run working notes, conditionally replaced.
    ContextCommit,
    /// Scoped private model invocation; never automatically redispatched.
    AuthoringModel,
    /// Native proposal receipt for a private authoring attempt.
    AuthoringActivity,
    /// Retain an unexpired authoring slot.
    AuthoringRenew,
    /// Independent authoring accounting and optional answer.
    AuthoringReport,
    /// More time on a held lease.
    Renew,
    /// Live-tail frames for a held lease.
    Activity,
    /// A lease's terminal result.
    Report,
    /// A fleet's memory at lease start.
    Hydrate,
    /// A fleet's memory written back.
    Capture,
    /// A search of a fleet's memory past the window.
    Recall,
    /// A scoped credential for a held lease.
    Mint,
    /// Finished calls' full records for a held lease.
    Records,
    /// The runner's own row: which runner this is, as the daemon names it.
    Me,
    /// A schedule created for the leased fleet.
    ScheduleCreate,
    /// The leased fleet's schedules.
    ScheduleList,
    /// A change to a schedule the fleet made.
    ScheduleUpdate,
    /// A schedule the fleet made, deleted.
    ScheduleDelete,
    /// A run of a schedule, created now.
    ScheduleRun,
    /// A schedule's runs.
    ScheduleRuns,
    /// A line said to the leased event's thread.
    Message,
    /// The messages a person sent while the run works, claimed for it.
    Folds,
    /// Complete native human input and recovered meaning.
    TaskInputs,
    /// One complete immutable human interpretation.
    TaskBindings,
    /// Accepted input application or original reply.
    TaskInputReceipt,
    /// One native physical decision reservation.
    TaskDecisionReserve,
    /// Original-owner physical decision accounting.
    TaskDecisionRecord,
    /// One exact-head native publication.
    TaskEffectPublish,
    /// Recovery effects beyond a confirmed watermark.
    TaskEffects,
    /// One retained publication receipt.
    TaskEffect,
    /// An immutable capture plan and writer pin.
    TaskCheckpointPlan,
    /// A complete verified manifest publication.
    TaskCheckpointCommit,
    /// An immutable manifest page.
    TaskCheckpointPageWrite,
    /// One authorized recovery page.
    TaskCheckpointPage,
    /// Bounded native plaintext object registration.
    TaskCheckpointObjectsRegister,
    /// Immutable ciphertext and create-only transfers.
    TaskCheckpointUploads,
    /// Native restore custody and transfer refresh.
    TaskCheckpointAuthorizations,
    /// Versioned native reader or writer pin renewal.
    TaskCheckpointPin,
}

impl Verb {
    /// Every verb, so a suite walks the whole vocabulary rather than a
    /// hand-kept list that a new verb can miss.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 40] = [
        Self::Heartbeat,
        Self::ContextLoad,
        Self::ContextCommit,
        Self::AuthoringModel,
        Self::AuthoringActivity,
        Self::AuthoringRenew,
        Self::AuthoringReport,
        Self::Renew,
        Self::Activity,
        Self::Report,
        Self::Hydrate,
        Self::Capture,
        Self::Recall,
        Self::Mint,
        Self::Records,
        Self::Me,
        Self::ScheduleCreate,
        Self::ScheduleList,
        Self::ScheduleUpdate,
        Self::ScheduleDelete,
        Self::ScheduleRun,
        Self::ScheduleRuns,
        Self::Message,
        Self::Folds,
        Self::TaskInputs,
        Self::TaskBindings,
        Self::TaskInputReceipt,
        Self::TaskDecisionReserve,
        Self::TaskDecisionRecord,
        Self::TaskEffectPublish,
        Self::TaskEffects,
        Self::TaskEffect,
        Self::TaskCheckpointPlan,
        Self::TaskCheckpointCommit,
        Self::TaskCheckpointPageWrite,
        Self::TaskCheckpointPage,
        Self::TaskCheckpointObjectsRegister,
        Self::TaskCheckpointUploads,
        Self::TaskCheckpointAuthorizations,
        Self::TaskCheckpointPin,
    ];

    /// The method this verb is sent with.
    pub(crate) const fn method(self) -> Method {
        match self {
            Self::Hydrate
            | Self::Me
            | Self::ScheduleList
            | Self::ScheduleRuns
            | Self::ContextLoad
            | Self::TaskInputs
            | Self::TaskEffects
            | Self::TaskEffect
            | Self::TaskCheckpointPage => Method::Get,
            Self::ContextCommit | Self::TaskCheckpointPageWrite => Method::Put,
            Self::ScheduleUpdate
            | Self::TaskDecisionRecord
            | Self::TaskCheckpointCommit
            | Self::TaskCheckpointPin => Method::Patch,
            Self::ScheduleDelete => Method::Delete,
            Self::Heartbeat
            | Self::AuthoringModel
            | Self::AuthoringActivity
            | Self::AuthoringRenew
            | Self::AuthoringReport
            | Self::Renew
            | Self::Activity
            | Self::Report
            | Self::Capture
            | Self::Recall
            | Self::Mint
            | Self::Records
            | Self::ScheduleCreate
            | Self::ScheduleRun
            | Self::Message
            | Self::Folds
            | Self::TaskBindings
            | Self::TaskInputReceipt
            | Self::TaskDecisionReserve
            | Self::TaskEffectPublish
            | Self::TaskCheckpointPlan
            | Self::TaskCheckpointObjectsRegister
            | Self::TaskCheckpointUploads
            | Self::TaskCheckpointAuthorizations => Method::Post,
        }
    }

    /// The verb as a log line and an error name it.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Heartbeat => "heartbeat",
            Self::ContextLoad => "context_load",
            Self::ContextCommit => "context_commit",
            Self::AuthoringModel => "authoring_model",
            Self::AuthoringActivity => "authoring_activity",
            Self::AuthoringRenew => "authoring_renew",
            Self::AuthoringReport => "authoring_report",
            Self::Renew => "renew",
            Self::Activity => "activity",
            Self::Report => "report",
            Self::Hydrate => "hydrate",
            Self::Capture => "capture",
            Self::Recall => "recall",
            Self::Mint => "mint",
            Self::Records => "records",
            Self::Me => "me",
            Self::ScheduleCreate => "schedule_create",
            Self::ScheduleList => "schedule_list",
            Self::ScheduleUpdate => "schedule_update",
            Self::ScheduleDelete => "schedule_delete",
            Self::ScheduleRun => "schedule_run",
            Self::ScheduleRuns => "schedule_runs",
            Self::Message => "message",
            Self::Folds => "folds",
            Self::TaskInputs => "task_inputs",
            Self::TaskBindings => "task_bindings",
            Self::TaskInputReceipt => "task_input_receipt",
            Self::TaskDecisionReserve => "task_decision_reserve",
            Self::TaskDecisionRecord => "task_decision_record",
            Self::TaskEffectPublish => "task_effect_publish",
            Self::TaskEffects => "task_effects",
            Self::TaskEffect => "task_effect",
            Self::TaskCheckpointPlan => "task_checkpoint_plan",
            Self::TaskCheckpointCommit => "task_checkpoint_commit",
            Self::TaskCheckpointPageWrite => "task_checkpoint_page_write",
            Self::TaskCheckpointPage => "task_checkpoint_page",
            Self::TaskCheckpointObjectsRegister => "task_checkpoint_objects_register",
            Self::TaskCheckpointUploads => "task_checkpoint_uploads",
            Self::TaskCheckpointAuthorizations => "task_checkpoint_authorizations",
            Self::TaskCheckpointPin => "task_checkpoint_pin",
        }
    }

    /// The registry code a failure of this verb is logged under.
    pub(crate) const fn code(self) -> ErrorCode {
        match self {
            Self::Hydrate | Self::Capture | Self::Recall => error_code::MEM_UNAVAILABLE,
            Self::ContextLoad
            | Self::ContextCommit
            | Self::Heartbeat
            | Self::AuthoringModel
            | Self::AuthoringActivity
            | Self::AuthoringRenew
            | Self::AuthoringReport
            | Self::Renew
            | Self::Activity
            | Self::Report
            | Self::Mint
            | Self::Records
            | Self::Me
            | Self::ScheduleCreate
            | Self::ScheduleList
            | Self::ScheduleUpdate
            | Self::ScheduleDelete
            | Self::ScheduleRun
            | Self::ScheduleRuns
            | Self::Message
            | Self::Folds
            | Self::TaskInputs
            | Self::TaskBindings
            | Self::TaskInputReceipt
            | Self::TaskDecisionReserve
            | Self::TaskDecisionRecord
            | Self::TaskEffectPublish
            | Self::TaskEffects
            | Self::TaskEffect
            | Self::TaskCheckpointPlan
            | Self::TaskCheckpointCommit
            | Self::TaskCheckpointPageWrite
            | Self::TaskCheckpointPage
            | Self::TaskCheckpointObjectsRegister
            | Self::TaskCheckpointUploads
            | Self::TaskCheckpointAuthorizations
            | Self::TaskCheckpointPin => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

impl fmt::Display for Verb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
