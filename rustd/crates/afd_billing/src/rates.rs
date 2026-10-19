//! What a run is priced at, and the estimate the credit gate checks.
//!
//! # Two postures, and only one of them needs the catalogue
//!
//! Under `self_managed` the tenant's own provider bills the tokens, so this
//! product charges the run fee and nothing else — resolvable with no statement
//! at all. Under `platform` the token cost is ours to charge, so the rate comes
//! from the catalogue. Splitting that at the top means the cheap posture stays
//! provable without a database.
//!
//! # An estimate is not a charge
//!
//! This module prices the ISSUE-time gate, and the gate's whole posture follows
//! from that sentence. A rate that cannot be established here must not refuse a
//! lease: the run has not happened, nothing is being billed, and a catalogue
//! that has moved out from under a fleet's model is an operational state rather
//! than a reason to stop the platform. So an unpriceable model answers
//! [`Estimate::Unpriceable`] and the gate admits.
//!
//! The same failure at renew or settle fails CLOSED, because there the number
//! IS the charge. That inversion is why pricing returns a value describing what
//! happened rather than a bare `Option` that has already chosen for both.

use afd_wire::usage::UsageLine;
use jiff::Timestamp;

use crate::error::Result;
use crate::price::{PriceBook, Rater, Window};
use crate::sql;
use crate::store::Accounts;
use crate::{ESTIMATE_FLOOR_INPUT_TOKENS, ESTIMATE_FLOOR_OUTPUT_TOKENS, Nanos};

/// Who supplies the provider key, and therefore who pays for tokens.
///
/// Parsing is deliberately exact and fallible: the column is written only by
/// this codebase, so an unknown spelling is a data-integrity fault to surface
/// rather than a value to guess at. A guess of `platform` for every
/// unrecognised string would silently attribute a self-managed run to platform
/// spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posture {
    /// The platform supplies the key; token cost is charged here.
    Platform,
    /// The tenant supplies the key; only the run fee is charged.
    SelfManaged,
}

impl Posture {
    /// The stored and wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Platform => sql::posture::PLATFORM,
            Self::SelfManaged => sql::posture::SELF_MANAGED,
        }
    }

    /// Recover a posture from its stored spelling.
    ///
    /// `None` for anything else — see the type's own note on why this refuses
    /// to guess.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        match stored {
            sql::posture::PLATFORM => Some(Self::Platform),
            sql::posture::SELF_MANAGED => Some(Self::SelfManaged),
            _unknown => None,
        }
    }
}

/// What the issue-time gate will check a balance against.
///
/// Three answers rather than two, because "the catalogue has no rate for this
/// model" is not the same as "the run costs nothing" and must not be folded
/// into `Priced(ZERO)` — that would silently admit against a floor of zero and
/// look identical to a correctly-priced free run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estimate {
    /// The floor cost of this run, in nanos.
    Priced(Nanos),
    /// The catalogue authoritatively carries no rate for this `(provider,
    /// model)` pair. The gate admits — an estimate is not a charge.
    Unpriceable,
}

impl Estimate {
    /// The cost to check a balance against.
    ///
    /// An unpriceable model costs nothing to ADMIT against, which is the
    /// fail-open posture stated as a value rather than performed by a `catch`.
    #[must_use]
    pub const fn floor(self) -> Nanos {
        match self {
            Self::Priced(nanos) => nanos,
            Self::Unpriceable => Nanos::ZERO,
        }
    }
}

impl Accounts {
    /// The floor cost of one run under `posture` at `now`, for the credit gate.
    ///
    /// Prices the conservative token floor through the same
    /// [`Rater`] that prices every usage line, and no run fee: the runner does
    /// not know its real token counts at lease time, and no time has elapsed
    /// yet. A self-managed run costs nothing at issue, so it reads no
    /// catalogue.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A model the catalogue does
    /// not price at `now` is [`Estimate::Unpriceable`], not an error — the two
    /// get different treatment and collapsing them would make a missing rate
    /// indistinguishable from a dead datastore.
    pub async fn estimate(
        &self,
        posture: Posture,
        provider: &str,
        model: &str,
        now: Timestamp,
    ) -> Result<Estimate> {
        if posture == Posture::SelfManaged {
            return Ok(Estimate::Priced(Nanos::ZERO));
        }
        let mut connection = self.pool().acquire().await?;
        let book = PriceBook::load(&mut *connection, provider, model).await?;
        floor(&book, now)
    }
}

/// The issue-time floor priced against `book` at `now`.
fn floor(book: &PriceBook, now: Timestamp) -> Result<Estimate> {
    let line = UsageLine {
        seq: 0,
        at: now,
        input_tokens: ESTIMATE_FLOOR_INPUT_TOKENS,
        cached_input_tokens: 0,
        cache_write_tokens: 0,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: ESTIMATE_FLOOR_OUTPUT_TOKENS,
        runtime_ms: 0,
    };
    let window = Window {
        from: now,
        until: now,
    };
    let rated = Rater::new(book, Posture::Platform, window).rate(&line)?;
    Ok(match rated.price {
        Some(_tier) => Estimate::Priced(rated.charged),
        None => Estimate::Unpriceable,
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Estimate, Posture};
    use crate::Nanos;

    #[test]
    fn an_empty_book_leaves_the_floor_unpriceable() {
        // A model the catalogue does not carry admits against a floor of zero,
        // and says so rather than reading as a free run.
        let now = jiff::Timestamp::UNIX_EPOCH;
        let estimate = super::floor(&crate::price::PriceBook::default(), now)
            .expect("an empty book prices without overflow");
        assert_eq!(estimate, Estimate::Unpriceable);
    }

    #[test]
    fn an_unpriceable_model_costs_nothing_to_admit_against() {
        // The fail-open posture, expressed as a value rather than performed by
        // a `catch`: a balance of zero still covers a floor of zero, so a
        // catalogue that moved out from under a fleet does not refuse its
        // lease.
        assert_eq!(Estimate::Unpriceable.floor(), Nanos::ZERO);
        assert!(Nanos::ZERO.covers(Estimate::Unpriceable.floor()));
    }

    #[test]
    fn a_priced_estimate_carries_its_floor_through() {
        let priced = Estimate::Priced(Nanos::from_i64(1_800));
        assert_eq!(priced.floor(), Nanos::from_i64(1_800));
        // And a balance under it does not cover it — the refusal the gate is for.
        assert!(!Nanos::from_i64(1_799).covers(priced.floor()));
    }

    #[test]
    fn a_posture_round_trips_through_its_stored_spelling() {
        for posture in [Posture::Platform, Posture::SelfManaged] {
            assert_eq!(Posture::parse(posture.as_str()), Some(posture));
        }
        // And an unknown spelling refuses rather than defaulting to platform,
        // which would attribute a self-managed run to platform spend.
        assert_eq!(Posture::parse("byo"), None);
        assert_eq!(Posture::parse(""), None);
    }
}
