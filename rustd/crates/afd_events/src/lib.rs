//! The narrative log of a fleet: the rows a run writes, the history an operator
//! reads, and the live tail they watch it on.
//!
//! # Why this is not a module inside `afd_fleet`
//!
//! `core.fleet_events` had no owner. Its statements lived in `afd_fleet::sql`,
//! beside leases, money, policy, bundles and four sweepers — so a crate wanting
//! ten lines of SQL had to depend on 25,500 lines of runner plane to get them.
//! `afd_approval` proved the cost: rather than pay it, it carried a
//! byte-identical COPY of the insert, and a third copy of the status spelling
//! beside it.
//!
//! This crate is the table's owner instead. `afd_fleet` and `afd_approval` both
//! depend on it, both run one text, and neither pulls the other in.
//!
//! # Writes are statements here, reads are verbs
//!
//! A write binds a caller's domain type — an acquired lease, a gate's refusal,
//! a runner's verdict — and those types belong to the planes that own them.
//! Moving the binding down here would drag them with it and invert the
//! layering, so [`sql`] exports the text and the writers keep their `bind`
//! chains.
//!
//! A read binds nothing from anywhere: a filter, a cursor, a limit. So the
//! reads are methods, and the statements behind them stay private.
//!
//! # The column vocabulary is lower still
//!
//! `status` and `failure_label` spellings live in [`afd_wire::event`], not
//! here, because they cross the wire — an events listing renders both. This
//! crate reads them from there like everybody else, which is what stops a row
//! one plane writes from being one another cannot recognise.

mod error;
mod history;
pub mod sql;

pub use self::error::{Error, Result};
pub use self::history::{
    Cursor, DEFAULT_LIMIT, EventRow, Filter, History, MAX_LIMIT, glob_to_like, next_cursor,
    parse_since,
};
