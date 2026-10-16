//! Authoring machine mutations share the same live subject and lease fence.

mod claim;
pub(crate) mod failure;
pub(crate) mod guard;
mod meter;
mod renew;
pub(crate) mod report;
pub(crate) mod sql;

use afd_fleet_runtime::definition::{ConfigOp, Definition, InstructionEdit};

pub use claim::Claimed;

/// Machine proposals use the drawer's exact Definition and edit types.
pub type ActivityInput =
    afd_wire::authoring::ActivityRequest<Definition, ConfigOp, InstructionEdit>;

pub(crate) type Proposal = afd_wire::authoring::Proposal<Definition, ConfigOp, InstructionEdit>;
pub(crate) type Event = afd_wire::authoring::ActivityEvent<Definition, ConfigOp, InstructionEdit>;
#[derive(Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Active,
    AwaitingAccounting,
    Settled,
}

impl Status {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::AwaitingAccounting => "awaiting_accounting",
            Self::Settled => "settled",
        }
    }
}
