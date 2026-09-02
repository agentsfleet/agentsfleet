//! Every family the census declares, as a value with its kind in the type.
//!
//! # Why this exists beside the census rather than instead of it
//!
//! The census is the contract and stays the single source of truth: it carries
//! the unit, the temporality, the bucket bounds, the series policy and the
//! label columns, and the registry is built FROM it. What a `.tsv` cannot do
//! is be referenced by a producer. A call site needs something to name, and if
//! that something is a string literal then every producer is one typo away
//! from feeding a family nobody declared — silently, because a metric backend
//! accepts whatever arrives.
//!
//! So each family also exists here as a `Declared<K>`: the name once, the kind
//! in the type. A producer names the constant, [`super::instrument::Instruments`]
//! looks it up in the census, and the two are held against each other at boot.
//!
//! # Grouped by who reads them, not by who writes them
//!
//! The modules are an operator's grouping. Someone looking at a stalled fleet
//! reads [`fleet`] end to end; someone looking at a slow catalogue reads
//! [`library`]. Grouping by producing crate would scatter each of those across
//! four files, and nothing reads them that way.

pub mod cost;
pub mod fleet;
pub mod http;
pub mod library;
pub mod memory;
pub mod redis;
