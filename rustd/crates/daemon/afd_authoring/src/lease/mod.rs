//! Authoring machine mutations share the same live subject and lease fence.

mod claim;
pub(crate) mod failure;
pub(crate) mod guard;
pub(crate) mod meter;
pub(crate) mod pending;
mod renew;
pub(crate) mod report;
pub(crate) mod sql;

use af_fleet_runtime::definition::{ConfigOp, Definition, InstructionEdit};

pub use claim::Claimed;

/// Machine proposals use the drawer's exact Definition and edit types.
pub type ActivityInput = af_wire::authoring::ActivityRequest<Definition, ConfigOp, InstructionEdit>;

pub(crate) type Proposal = af_wire::authoring::Proposal<Definition, ConfigOp, InstructionEdit>;
pub(crate) type Event = af_wire::authoring::ActivityEvent<Definition, ConfigOp, InstructionEdit>;
#[derive(Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Active,
    AwaitingAccounting,
    Settled,
}

/// Statuses of an attempt that no longer holds its draft: finished, whether or
/// not its usage is accounted. Bound as one array so the draft's sequence index,
/// not the status index, answers "is a turn still running here".
pub(crate) const FINISHED: [&str; 2] = [
    Status::AwaitingAccounting.as_str(),
    Status::Settled.as_str(),
];

impl Status {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::AwaitingAccounting => "awaiting_accounting",
            Self::Settled => "settled",
        }
    }
}
