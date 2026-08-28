//! The operator's side of an approval gate.
//!
//! # Why this is not a module inside `afd_fleet`
//!
//! `afd_gate::gate` is the RUNNER's side: it parks a run behind a gate and
//! reads the durable answer back. This crate is the PERSON's side, and the two
//! ask different questions of one table — a runner asks about one action it
//! already holds, an operator browses a queue they did not raise and answers
//! rows they must be authorised for.
//!
//! Keeping them apart is what lets the API's approval surface compile without
//! the runner plane behind it: `afd_fleet` carries leases, money, policy,
//! bundles, sweepers and the vault reader, and none of that is on the path of
//! showing a person what a fleet wants to do.
//!
//! What the two DO share is one column's vocabulary, and that lives lower down
//! in [`afd_wire::approval::status`] where both read it — a drift between two
//! copies would make a row one plane wrote the other could not read.
//!
//! # The race is Postgres's decision
//!
//! Two operators answering one gate both run one UPDATE carrying
//! `WHERE status = 'pending'`. Exactly one updates a row; the other's
//! `RETURNING` comes back empty, which is how [`Resolution`] tells "you decided
//! this" from "somebody already had". A read-then-write would let both believe
//! they won and both tell their person so.

mod decision;
mod error;
mod inbox;
mod sql;

pub use self::decision::Decision;
pub use self::error::{Error, Result};
pub use self::inbox::{Cursor, Filter, GateRow, Inbox, Resolution, Resolved};
