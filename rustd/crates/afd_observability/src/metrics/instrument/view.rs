//! Each family's series ceiling, applied where the SDK can enforce it.
//!
//! # Why the ceiling is a view and not a check at the call site
//!
//! The census declares how many series a family may occupy. A producer cannot
//! enforce that — it sees one measurement at a time and has no idea how many
//! distinct label sets the family has accumulated. The SDK does know, and a
//! view is where it takes the number.
//!
//! # Why the streams are built twice
//!
//! A view answers `Option<Stream>`, so a stream the SDK REFUSES is
//! indistinguishable, from inside the closure, from "this view does not apply
//! to this instrument" — and the family would then export under default
//! buckets with nobody told. So every stream is built once eagerly here, where
//! a refusal is an [`Error`] naming the family, and the closure rebuilds the
//! same configuration for the instruments that arrive later. The `ok()` there
//! discards an error that construction already proved cannot happen.

use std::collections::BTreeMap;

use opentelemetry_sdk::metrics::{Instrument, Stream};

use crate::error::{Error, Result};
use crate::metrics::registry::{Family, Policy, Registry};

/// A view applying every declared family's series ceiling.
///
/// A family drawing on the shared cost sub-budget gets none: it owns no
/// ceiling of its own, so the SDK's default applies and the budget is held
/// where the cost families are attributed rather than per stream.
///
/// # Errors
///
/// [`Error::StreamRejected`] when the SDK will not accept the ceiling a family
/// declares — a zero, or a number it cannot size its own table from.
pub fn series_ceilings(
    registry: &Registry,
) -> Result<impl Fn(&Instrument) -> Option<Stream> + Send + Sync + 'static> {
    let mut ceilings: BTreeMap<Box<str>, usize> = BTreeMap::new();
    for family in registry.families() {
        let Some(limit) = ceiling_of(family) else {
            continue;
        };
        if let Err(reason) = build(limit) {
            return Err(Error::StreamRejected {
                family: family.name.clone(),
                reason,
            });
        }
        ceilings.insert(family.name.clone(), limit);
    }

    Ok(move |instrument: &Instrument| {
        let limit = *ceilings.get(instrument.name())?;
        // Proved buildable above, for this exact number.
        build(limit).ok()
    })
}

/// The ceiling a family declares, or nothing when it owns none.
const fn ceiling_of(family: &Family) -> Option<usize> {
    match family.policy {
        Policy::Fixed { max_series } => Some(max_series),
        Policy::Runner { slots } => Some(slots),
        Policy::SharedCost => None,
    }
}

/// One stream carrying nothing but a ceiling.
///
/// Nothing else is set, deliberately. A view that also restated a family's
/// name, unit or aggregation would be a second declaration of facts the census
/// already carries and the instrument was already built with.
fn build(limit: usize) -> core::result::Result<Stream, Box<str>> {
    match Stream::builder().with_cardinality_limit(limit).build() {
        Ok(stream) => Ok(stream),
        // The sentence, as DATA, and the one place this crate does that.
        // `Error::StreamRejected` carries the reasoning: the SDK's refusal is a
        // `Box<dyn Error>` that is not `Send + Sync`, so it cannot be held in
        // an error of ours, and in this version it is always built from a
        // `&'static str` with no cause of its own. There is no `source()` chain
        // to lose here — written as a `map_err` it would read like the lossy
        // conversion the standard bans, which is why it is not one.
        Err(refusal) => Err(refusal.to_string().into_boxed_str()),
    }
}
