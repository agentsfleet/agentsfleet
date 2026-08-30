//! The declared metric families, deserialized from the contract that declares
//! them.
//!
//! `docs/metrics.census.tsv` is the single source of truth for the export, and
//! this module turns it into the typed set the instrument layer builds from.
//!
//! # Why there is no parser here
//!
//! The columns are read by `csv` and the tokens by `serde`, so the closed
//! vocabularies live on the enums as `#[serde(rename)]` rather than in a match
//! arm somewhere else that can drift from them. What that buys is not brevity:
//! an unknown `kind` now fails with the record, the line, the field and the
//! full set of accepted spellings, which is a better message than a
//! hand-written parser would have carried, and it cannot fall out of step with
//! the type because it IS the type.
//!
//! What stays hand-written is only what neither crate can know — that two rows
//! must not name one family, and that a kind and its bucket bounds have to
//! agree (`M-STRONG-TYPES`: both are checked once, here, so nothing downstream
//! re-checks them).

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::metrics::family::{Counter, Gauge, Histogram};

pub use self::column::{Category, Kind, Number, ParsePolicy, Policy, Temporality};

mod column;

#[cfg(test)]
mod tests;

/// The contract itself, compiled in. Read at build time so a daemon can never
/// disagree with the file the parity test grades it against.
pub const CENSUS: &str = include_str!("../../../../../docs/metrics.census.tsv");

/// One declared family: everything the wire needs and nothing it does not.
///
/// `PartialEq` without `Eq` because bucket bounds are `f64`. That is not an
/// oversight papered over: bounds are read from a text contract and compared
/// against it, which is exactly where float equality is meaningful, while a
/// total order over them is not something this type should claim.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Family {
    /// The wire name, byte-exact. This is the parity surface.
    pub name: Box<str>,
    /// What the instrument accumulates.
    pub kind: Kind,
    /// The number type it records in.
    pub number: Number,
    /// The UCUM unit string, byte-exact.
    pub unit: Box<str>,
    /// Cumulative or delta; absent for a gauge, which has no window.
    #[serde(deserialize_with = "column::absent_as_none")]
    pub temporality: Option<Temporality>,
    /// The label keys, in declaration order.
    #[serde(deserialize_with = "column::comma_separated")]
    pub labels: Vec<Box<str>>,
    /// Histogram bucket bounds in `unit`; empty for every other kind.
    #[serde(deserialize_with = "column::comma_separated_numbers")]
    pub bounds: Vec<f64>,
    /// The series ceiling and its basis.
    #[serde(deserialize_with = "column::from_str")]
    pub policy: Policy,
    /// Whether the value is published into a snapshot and read by a callback.
    #[serde(deserialize_with = "column::yes_no")]
    pub live_read: bool,
    /// The operator legend this family is read under.
    pub category: Category,
    /// One line of operator guidance. Prose, so nothing parses it.
    pub watch_for: Box<str>,
}

/// Every family the contract declares, keyed by wire name.
///
/// `BTreeMap` rather than a hash map so iteration is wire-name order: a drift
/// message names families in the same sequence on every run, and a reader can
/// find a row in the census by the order they were printed in.
#[derive(Debug, Clone)]
pub struct Registry {
    families: BTreeMap<Box<str>, Family>,
}

impl Registry {
    /// Reads the compiled-in contract.
    ///
    /// # Errors
    ///
    /// Any row the reader or the closed vocabularies reject, naming its line;
    /// a family declared twice; a kind and bounds that contradict each other.
    pub fn declared() -> Result<Self> {
        Self::read(CENSUS)
    }

    /// Reads a census, so a test can feed it a seeded-wrong one.
    ///
    /// # Errors
    ///
    /// As [`Registry::declared`].
    pub fn read(census: &str) -> Result<Self> {
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b'\t')
            .comment(Some(b'#'))
            .from_reader(census.as_bytes());

        // Records rather than `deserialize()`, because a `StringRecord` carries
        // the position it was read from and the typed iterator does not — and a
        // duplicate has to name both lines to be actionable.
        let headers = reader.headers()?.clone();
        let mut families = BTreeMap::new();
        let mut lines: BTreeMap<Box<str>, u64> = BTreeMap::new();

        for record in reader.records() {
            let record = record?;
            let line = record.position().map_or(0, csv::Position::line);
            let family: Family = record.deserialize(Some(&headers))?;
            check_bounds_agree_with_kind(&family)?;

            if let Some(first) = lines.get(&family.name) {
                return Err(Error::Duplicate {
                    family: family.name.clone(),
                    first: *first,
                    second: line,
                });
            }
            lines.insert(family.name.clone(), line);
            families.insert(family.name.clone(), family);
        }

        Ok(Self { families })
    }

    /// The family declared under `name`.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownFamily`] when the contract declares no such name.
    pub fn family(&self, name: &str) -> Result<&Family> {
        self.families.get(name).ok_or_else(|| Error::UnknownFamily {
            family: name.into(),
        })
    }

    /// The census entry for a family whose type claims it is a counter.
    ///
    /// Two independent checks meet here, which is the point of the trait
    /// layer. The `M: Counter` bound is settled by the compiler: a type that
    /// does not implement [`Counter`] cannot be passed at all, so there is no
    /// run-time path for "recorded a gauge as a counter". The census check is
    /// the other half — the contract on disk must agree with what the type
    /// claims, and a disagreement is a defect in one of the two that nothing
    /// else would catch.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownFamily`] when the census declares no such name, or
    /// [`Error::KindMismatch`] when it declares it a different kind.
    pub fn counter<M: Counter>(&self, family: &M) -> Result<&Family> {
        self.of_kind(family.name(), Kind::Counter)
    }

    /// The census entry for a family whose type claims it is a histogram.
    ///
    /// # Errors
    ///
    /// As [`Registry::counter`].
    pub fn histogram<M: Histogram>(&self, family: &M) -> Result<&Family> {
        self.of_kind(family.name(), Kind::Histogram)
    }

    /// The census entry for a family whose type claims it is a gauge.
    ///
    /// # Errors
    ///
    /// As [`Registry::counter`].
    pub fn gauge<M: Gauge>(&self, family: &M) -> Result<&Family> {
        self.of_kind(family.name(), Kind::Gauge)
    }

    /// The shared half of the three above: found, and of the claimed kind.
    fn of_kind(&self, name: &str, claimed: Kind) -> Result<&Family> {
        let family = self.family(name)?;
        if family.kind == claimed {
            return Ok(family);
        }
        Err(Error::KindMismatch {
            family: family.name.clone(),
            declared: family.kind.spelling(),
            claimed: claimed.spelling(),
        })
    }

    /// Every declared family, in wire-name order.
    #[must_use]
    pub fn families(&self) -> impl ExactSizeIterator<Item = &Family> {
        self.families.values()
    }

    /// How many families the contract declares.
    #[must_use]
    pub fn len(&self) -> usize {
        self.families.len()
    }

    /// Whether the contract declares nothing, which is itself a defect.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.families.is_empty()
    }
}

/// Exactly the histograms carry bounds. Checked once, so no instrument builder
/// downstream has to ask again (`M-STRONG-TYPES`).
fn check_bounds_agree_with_kind(family: &Family) -> Result<()> {
    if family.kind.takes_bounds() != family.bounds.is_empty() {
        return Ok(());
    }
    Err(Error::BoundsMismatch {
        family: family.name.clone(),
        kind: family.kind.spelling(),
        bounds: family.bounds.len(),
    })
}
