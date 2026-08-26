//! Lease issue: the claim, the gates, the money, and the row.
//!
//! The `lease` verb performs the pre-execution control-plane work and hands a
//! self-contained `ExecutionPolicy` to the runner. Split by concern the way
//! [`crate::runner`] is: [`affinity`] is the atomic claim and the fence it
//! mints — the only place a [`affinity::Fence`] comes from — and the modules
//! beside it add the gates and the row.
//!
//! # What the Zig splits on, and why this does not
//!
//! `fleet/service.zig` and `fleet/service_billing.zig` are one logical module
//! carved in two for the Zig file-length gate, with `service.zig` re-exporting
//! `Billed` so the halves keep naming one type. That is a workaround for a
//! gate, not a design. Here the split falls on what the code DOES — claim,
//! gate, issue — so no module needs a re-export to stay findable.

pub mod activity;
pub mod admit;
pub mod affinity;
mod answer;
pub mod assign;
pub mod coverage;
mod deliver;
pub mod envelope;
pub mod event;
pub mod finalize;
pub mod installed;
pub mod issue;
pub mod pull;
pub mod reclaim;
pub mod renew;
pub mod report;
pub mod settle;
pub mod store;
pub mod verdict;

pub use self::activity::Target;
pub use self::affinity::{Claimed, Fence};
pub use self::envelope::{Acquired, Kind};
pub use self::event::{Delivery, Ended};
pub use self::installed::{FRESH_CONTEXT, Installed};
pub use self::issue::{Billed, Issued};
pub use self::pull::Plane;
pub use self::reclaim::{Reclaimed, Reused};
pub use self::renew::{Renewed, Renewing};
pub use self::settle::{Reported, Settled};
pub use self::store::Leases;
pub use self::verdict::{Terminal, Verdict};
