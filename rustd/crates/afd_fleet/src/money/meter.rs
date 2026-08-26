//! What one metered slice is priced from: the runner's cumulative counts, and
//! the rates they are charged at.
//!
//! # What this replaces, and why it is not a transliteration
//!
//! `renewal.zig`'s `MeterInputs` is seven flat `i64` fields — three cumulative
//! token counts and four per-unit rates — because it exists to be splatted into
//! a positional query and Zig has nothing cheaper to group them with. Ported
//! literally that would put seven same-typed integers on one constructor, where
//! any two transposed compile clean and misprice every slice from then on.
//!
//! The four rates are already a type here ([`SliceRates`]), resolved once and
//! shared with the pure [`slice_charge`] reference. So this module supplies the
//! other half — [`Cumulative`] — and [`Meter`] is the pair. Two named fields
//! instead of seven positional ones, and the only way to build a `Cumulative`
//! is to name all three counts.
//!
//! # The fail-open posture is the CALLER's, not this module's
//!
//! `buildMeterInputs` swallows two different failures into one run-fee-only
//! answer and logs them apart: a catalogue generation that could not be read,
//! and a catalogue that authoritatively carries no such model. Only the second
//! is a fact about pricing. The first is a datastore fault, and returning a
//! priced meter for it means a slice was charged against rates nobody verified.
//!
//! [`Accounts::meter`] separates them the way [`super`] says this module
//! separates every gate: a MISS is a value (run-fee-only rates, which is the
//! honest price of a model the catalogue does not carry), and a FAULT is an
//! [`Error`](crate::Error). The decision to meter run-fee-only rather than kill
//! a live run over a transient fault is unchanged from the Zig — but it is made
//! once, at the verb, where it can be read, instead of inside the resolver.

use super::rates::Posture;
use super::{RUN_NANOS_PER_SEC, SliceRates};
use crate::error::Result;
use crate::money::store::Accounts;

/// The runner's cumulative token counts for the whole run.
///
/// CUMULATIVE, never deltas, and the type is what says so: the statements diff
/// these against the affinity cursor themselves, so a caller that helpfully
/// subtracted first would double-count every slice. The wire carries `u32` and
/// the columns are `bigint`, so the widening happens once, here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cumulative {
    /// Prompt tokens for the whole run so far.
    pub input: i64,
    /// Cache-read tokens for the whole run so far.
    pub cached: i64,
    /// Completion tokens for the whole run so far.
    pub output: i64,
}

impl Cumulative {
    /// The counts a runner reported, widened from the wire's `u32`.
    ///
    /// Takes all three by name at the only place they can be built, so the
    /// transposition the Zig's flat struct invites has one site to be wrong at
    /// rather than every call.
    #[must_use]
    pub fn reported(input: u32, cached: u32, output: u32) -> Self {
        Self {
            input: i64::from(input),
            cached: i64::from(cached),
            output: i64::from(output),
        }
    }
}

/// A slice's counts and the rates they price against.
///
/// Built once per verb and shared by renew and settle, so the two meter
/// identically by construction — the property `buildMeterInputs` exists to give
/// the Zig, kept, with the grouping the Zig could not express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meter {
    /// What the runner has counted so far.
    pub cumulative: Cumulative,
    /// What those counts, and the elapsed runtime, are charged at.
    pub rates: SliceRates,
}

/// The rates a model the catalogue does not carry is metered at.
///
/// Run fee only. Not a fallback and not a guess: the run genuinely happened and
/// its runtime is genuinely owed, while the token component is DROPPED rather
/// than invented, because nothing published a price for it. Naming it is what
/// keeps it from reading as an accident at the call site.
const RUN_FEE_ONLY: SliceRates = SliceRates {
    run_nanos_per_sec: RUN_NANOS_PER_SEC,
    input_nanos_per_mtok: 0,
    cached_input_nanos_per_mtok: 0,
    output_nanos_per_mtok: 0,
};

impl Accounts {
    /// What one slice of this run is metered at.
    ///
    /// Self-managed postures issue no statement — they never reach the
    /// catalogue, because the tenant's own provider bill carries the tokens and
    /// only the run fee is ours to charge.
    ///
    /// A model the catalogue authoritatively does not carry meters at
    /// [`RUN_FEE_ONLY`]. That is a priced answer, not a failure: the catalogue
    /// answered, and what it said was that there is no token price here.
    ///
    /// # Errors
    /// Reports a datastore that would not answer — which is NOT the same as a
    /// missing rate, and the two must not be collapsed. A caller that meters a
    /// slice against rates it could not verify has charged a tenant against a
    /// generation nobody read.
    pub async fn meter(
        &self,
        posture: Posture,
        provider: &str,
        model: &str,
        cumulative: Cumulative,
    ) -> Result<Meter> {
        let rates = self.slice_rates(posture, provider, model).await?;
        Ok(Meter { cumulative, rates })
    }

    /// The meter a caller falls back to when the catalogue could not be read.
    ///
    /// Pure — no statement, no `Result` — because it is the answer for the case
    /// where asking the datastore is what failed. Exposed rather than left for
    /// each caller to assemble from [`RUN_FEE_ONLY`], so the fail-open posture
    /// resolves to one set of rates wherever it is taken.
    #[must_use]
    pub const fn run_fee_meter(&self, cumulative: Cumulative) -> Meter {
        Meter {
            cumulative,
            rates: RUN_FEE_ONLY,
        }
    }

    /// The rates alone, for the caller that has no counts yet.
    async fn slice_rates(
        &self,
        posture: Posture,
        provider: &str,
        model: &str,
    ) -> Result<SliceRates> {
        match posture {
            Posture::SelfManaged => Ok(RUN_FEE_ONLY),
            Posture::Platform => Ok(self
                .catalogue_rates(provider, model)
                .await?
                .unwrap_or(RUN_FEE_ONLY)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cumulative, RUN_FEE_ONLY};
    use crate::money::nanos::MS_PER_SEC;

    /// A token count far above any rate this test publishes, so "adds nothing"
    /// is a statement about the RATE rather than about a small count.
    const TOKENS: i64 = 5_000_000;
    use crate::money::{RUN_NANOS_PER_SEC, slice_charge};

    /// The wire's three counts reach the three columns they name.
    ///
    /// Cheap, and it is the whole reason this type exists: the Zig's flat
    /// `MeterInputs` takes these as three positional `u32`s beside four
    /// positional `i64` rates, where a transposition prices every later slice
    /// wrong and nothing fails.
    #[test]
    fn test_reported_counts_reach_the_fields_they_name() {
        let counts = Cumulative::reported(1, 2, 3);
        assert_eq!(counts.input, 1, "prompt tokens land on input");
        assert_eq!(counts.cached, 2, "cache reads land on cached");
        assert_eq!(counts.output, 3, "completions land on output");
    }

    /// An unpriced model is charged for its runtime and for nothing else.
    ///
    /// The property that makes [`RUN_FEE_ONLY`] a price rather than a fallback:
    /// tokens contribute zero at any count, so a catalogue gap can never charge
    /// a tenant for tokens nobody published a rate for.
    #[test]
    fn test_run_fee_only_charges_runtime_and_no_tokens() {
        let one_second = slice_charge(RUN_FEE_ONLY, MS_PER_SEC, 0, 0, 0);
        assert_eq!(
            one_second.as_i64(),
            RUN_NANOS_PER_SEC,
            "a second of runtime costs one second of run fee"
        );
        let with_tokens = slice_charge(RUN_FEE_ONLY, MS_PER_SEC, TOKENS, TOKENS, TOKENS);
        assert_eq!(
            with_tokens, one_second,
            "fifteen million tokens add nothing at an unpublished rate"
        );
    }
}
