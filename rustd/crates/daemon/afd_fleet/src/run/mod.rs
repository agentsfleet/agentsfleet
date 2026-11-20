//! Run issue: the assignment, the gates, the money, and the row.
//!
//! The `run` verb performs the pre-execution control-plane work and hands a
//! self-contained `ExecutionPolicy` to the runner. Split by concern:
//! [`affinity`] holds the fence and the thread's release verbs — a fence is
//! minted by the thread upsert inside the assignment head statement
//! (`assign_turn_head.sql`) and read back into a [`affinity::Fence`] — and
//! the modules beside it add the gates and the row.
//!
//! # Where the modules split
//!
//! The split falls on what the code DOES — assign, gate, issue — rather than on
//! where a file grew too long, so no module needs a re-export to stay findable.

pub mod activity;
pub mod admit;
pub mod affinity;
mod answer;
pub mod assign;
mod assignment;
pub mod billing_sweep;
pub mod bracket;
pub mod cache;
pub mod commit;
pub mod coverage;
pub(crate) mod deadline;
mod decision;
mod deliver;
pub mod delivery;
pub mod draft;
pub mod envelope;
pub mod fence;
pub mod finalize;
pub mod fold;
pub mod history;
pub mod installed;
pub mod issue;
pub mod memory;
pub mod message;
pub mod mint;
pub mod owner;
mod park;
mod placement;
pub mod pull;
pub mod renew;
pub mod report;
mod runner_runs;
pub mod scope;
pub mod settle;
pub mod sql;
pub mod standing;
pub mod store;
#[cfg(all(test, feature = "test-util"))]
mod test_dead;
mod tool_detail;
mod tool_trace;
pub mod turn;
pub mod turn_checkpoint;
mod unanswered;
mod unsent;
pub mod usage;
pub mod verdict;
pub mod waiting;
pub mod write_fence;

pub use self::activity::Target;
pub use self::affinity::Fence;
#[cfg(any(test, feature = "test-util"))]
pub use self::affinity::WonAssignment;
pub use self::assignment::{AssignRequest, AssignedRun, RecoveryScan};
pub use self::commit::{Committed, TerminalReport};
pub use self::delivery::{Owing, ReplyDestination};
pub use self::envelope::{Acquired, Kind};
pub use self::history::Thread;
pub use self::installed::{FRESH_CONTEXT, Installed};
pub use self::issue::{Billed, Issued};
pub use self::owner::RunOwner;
pub use self::pull::RunService;
pub use self::renew::{Renewed, Renewing};
pub use self::scope::MintScope;
pub use self::settle::Reported;
pub use self::standing::Standing;
pub use self::store::Runs;
pub use self::turn::{Delivery, Ended, Received};
pub use self::verdict::{Terminal, Verdict};
