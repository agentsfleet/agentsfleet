//! What a fleet is allowed to do, and who had to say so.
//!
//! # Why this is not part of `afd_fleet`
//!
//! Two modules answer one question between them. [`policy`] assembles what a
//! fleet's configuration permits — which model, which network reach, which
//! repository a write may touch — and [`gate`] is where a run STOPS when the
//! answer needs a human: it parks the event, records what was proposed, and
//! reads the durable decision back.
//!
//! Neither asks anything of a lease. The lease plane calls DOWN into this one
//! at exactly two moments — admitting a claim and spending an approval — and
//! nothing here calls back.
//!
//! # This is the runner's side of an approval
//!
//! `afd_approval` is the OPERATOR's: the queue a person browses, the one
//! decision Postgres admits, the continuation an approval lands. This crate is
//! what parks the run in the first place and what reads the answer. The two
//! share one column's vocabulary, in [`afd_wire::approval::status`], and
//! nothing else.

mod error;

pub mod gate;
pub mod policy;

pub use self::error::{
    DETAIL_GATE_BINDING_UNWRITABLE, DETAIL_GATE_REFERENCE_UNWRITABLE, Error, Result,
};
