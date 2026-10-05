//! Every runner verb: its name, its method, and the code its failure is
//! logged under.
//!
//! Split from [`super`], which sends them, so the vocabulary reads in one
//! place: a verb added here without a method, a name or a code does not
//! compile.

use std::fmt;

use afd_core::error_code::{self, ErrorCode};

/// The HTTP method a verb is sent with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Method {
    /// A read.
    Get,
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
    /// The next event to run.
    Lease,
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
    /// A fleet bundle by content hash.
    Bundle,
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
}

impl Verb {
    /// Every verb, so a suite walks the whole vocabulary rather than a
    /// hand-kept list that a new verb can miss.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 19] = [
        Self::Heartbeat,
        Self::Lease,
        Self::Renew,
        Self::Activity,
        Self::Report,
        Self::Hydrate,
        Self::Capture,
        Self::Recall,
        Self::Bundle,
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
    ];

    /// The method this verb is sent with.
    pub(crate) const fn method(self) -> Method {
        match self {
            Self::Hydrate | Self::Bundle | Self::Me | Self::ScheduleList | Self::ScheduleRuns => {
                Method::Get
            }
            Self::ScheduleUpdate => Method::Patch,
            Self::ScheduleDelete => Method::Delete,
            Self::Heartbeat
            | Self::Lease
            | Self::Renew
            | Self::Activity
            | Self::Report
            | Self::Capture
            | Self::Recall
            | Self::Mint
            | Self::Records
            | Self::ScheduleCreate
            | Self::ScheduleRun
            | Self::Message => Method::Post,
        }
    }

    /// The verb as a log line and an error name it.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Heartbeat => "heartbeat",
            Self::Lease => "lease",
            Self::Renew => "renew",
            Self::Activity => "activity",
            Self::Report => "report",
            Self::Hydrate => "hydrate",
            Self::Capture => "capture",
            Self::Recall => "recall",
            Self::Bundle => "bundle",
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
        }
    }

    /// The registry code a failure of this verb is logged under.
    pub(crate) const fn code(self) -> ErrorCode {
        match self {
            Self::Bundle => error_code::FLEET_BUNDLE_FETCH_FAILED,
            Self::Hydrate | Self::Capture | Self::Recall => error_code::MEM_UNAVAILABLE,
            Self::Heartbeat
            | Self::Lease
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
            | Self::Message => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

impl fmt::Display for Verb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
