//! What a run is priced at, and the estimate the credit gate checks.
//!
//! # Two postures, and only one of them needs the catalogue
//!
//! Under `self_managed` the tenant's own provider bills the tokens, so this
//! product charges the run fee and nothing else — resolvable with no statement
//! at all. Under `platform` the token cost is ours to charge, so the rate comes
//! from the catalogue. Splitting that at the top means the cheap posture stays
//! provable without a database, which is `tenant_billing_rates.zig`'s own
//! reasoning and worth keeping.
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

use sqlx::Row as _;

use crate::error::{Result, query};
use crate::money::store::Accounts;
use crate::money::{
    ESTIMATE_FLOOR_INPUT_TOKENS, ESTIMATE_FLOOR_OUTPUT_TOKENS, Nanos, RUN_NANOS_PER_SEC,
    SliceRates, slice_charge,
};
use crate::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_RATE: &str = "model rate at revision";

/// No time has elapsed at lease issue, so the estimate carries no run fee.
///
/// Named rather than passed as a bare `0`: the run fee accrues per renewal once
/// the fleet is actually running, and a reader meeting `0` at this call site
/// would reasonably wonder whether it was a placeholder.
const ELAPSED_AT_ISSUE_MS: i64 = 0;

/// The cached-input token count the issue-time floor assumes.
///
/// Zero, and deliberately not a third floor constant: a fresh run has no cache
/// to hit, so assuming cached input would price the estimate BELOW what the
/// run will cost.
const CACHED_TOKENS_AT_ISSUE: i64 = 0;

/// Who supplies the provider key, and therefore who pays for tokens.
///
/// `tenant_provider.zig`'s `Mode`. Parsing is deliberately exact and fallible:
/// the column is written only by this codebase, so an unknown spelling is a
/// data-integrity fault to surface rather than a value to guess at. Guessing is
/// what the Zig's retired per-file helpers did — every unrecognised string
/// became `platform`, silently attributing a self-managed run to platform
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
            Self::Platform => sql::billing::posture::PLATFORM,
            Self::SelfManaged => sql::billing::posture::SELF_MANAGED,
        }
    }

    /// Recover a posture from its stored spelling.
    ///
    /// `None` for anything else — see the type's own note on why this refuses
    /// to guess.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        match stored {
            sql::billing::posture::PLATFORM => Some(Self::Platform),
            sql::billing::posture::SELF_MANAGED => Some(Self::SelfManaged),
            _unknown => None,
        }
    }

    /// The rates this posture prices at without consulting the catalogue.
    ///
    /// `None` means the catalogue is required, which is `Platform` and only
    /// `Platform`. Split out so the self-managed branch costs no statement and
    /// stays provable with no database in the test.
    ///
    /// If this ever answered `Some` for `Platform`, every metered slice would
    /// price from a constant — which is the exact defect that once made stage
    /// billing charge nothing at all.
    #[must_use]
    const fn rates_without_catalogue(self) -> Option<SliceRates> {
        match self {
            Self::SelfManaged => Some(SliceRates {
                run_nanos_per_sec: RUN_NANOS_PER_SEC,
                input_nanos_per_mtok: 0,
                cached_input_nanos_per_mtok: 0,
                output_nanos_per_mtok: 0,
            }),
            Self::Platform => None,
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
    /// The floor cost of one run under `posture`, for the credit gate.
    ///
    /// Prices the conservative token floor and no run fee: the runner does not
    /// know its real token counts at lease time, and no time has elapsed yet.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A model the catalogue does
    /// not carry is [`Estimate::Unpriceable`], not an error — the two get
    /// different treatment and collapsing them would make a missing rate
    /// indistinguishable from a dead datastore.
    pub async fn estimate(
        &self,
        posture: Posture,
        provider: &str,
        model: &str,
    ) -> Result<Estimate> {
        let rates = match posture.rates_without_catalogue() {
            Some(rates) => Some(rates),
            None => self.catalogue_rates(provider, model).await?,
        };
        Ok(rates.map_or(Estimate::Unpriceable, |rates| {
            Estimate::Priced(slice_charge(
                rates,
                ELAPSED_AT_ISSUE_MS,
                ESTIMATE_FLOOR_INPUT_TOKENS,
                CACHED_TOKENS_AT_ISSUE,
                ESTIMATE_FLOOR_OUTPUT_TOKENS,
            ))
        }))
    }

    /// The catalogue's rates for one `(provider, model)` pair.
    ///
    /// One statement, which returns the rate AND the generation it was read at
    /// in a single snapshot — so there is no window for the counter to advance
    /// between reading it and reading the rate, and no cache to hold a rate
    /// under a generation it does not belong to. See [`super::store`] for why
    /// the Zig's rate cache does not come across.
    ///
    /// `Ok(None)` is the join's null half: the revision row answered and the
    /// catalogue carried no matching model. The `LEFT JOIN` is driven from the
    /// singleton precisely so those two facts arrive together.
    async fn catalogue_rates(&self, provider: &str, model: &str) -> Result<Option<SliceRates>> {
        let mut connection = self.pool().acquire().await?;
        let row = sqlx::query(sql::billing::LOAD_RATE_WITH_REVISION)
            .bind(provider)
            .bind(model)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_RATE))?;

        // Column 1 is the context cap, and it is the join's presence witness:
        // NULL there means no catalogue row matched, whatever the rate columns
        // hold. Testing a rate column instead would read a genuinely zero-rated
        // model as absent.
        let present: Option<i32> = row.try_get(1).map_err(query(CONTEXT_RATE))?;
        if present.is_none() {
            return Ok(None);
        }
        let tier = |index: usize| -> Result<i64> {
            row.try_get::<Option<i64>, _>(index)
                .map(|value| value.unwrap_or(0))
                .map_err(query(CONTEXT_RATE))
        };
        Ok(Some(SliceRates {
            run_nanos_per_sec: RUN_NANOS_PER_SEC,
            input_nanos_per_mtok: tier(2)?,
            cached_input_nanos_per_mtok: tier(3)?,
            output_nanos_per_mtok: tier(4)?,
        }))
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Estimate, Posture};
    use crate::money::{Nanos, RUN_NANOS_PER_SEC, slice_charge};

    #[test]
    fn only_the_platform_posture_needs_the_catalogue() {
        // The property that keeps the self-managed branch database-free. If
        // `Platform` ever answered rates here it would be pricing from a
        // constant, which is how stage billing once charged nothing at all.
        assert!(Posture::Platform.rates_without_catalogue().is_none());
        assert!(Posture::SelfManaged.rates_without_catalogue().is_some());
    }

    #[test]
    fn a_self_managed_estimate_at_issue_is_zero() {
        // No elapsed time and no token charge: the run fee has not started
        // accruing and the tenant's own provider bills the tokens. So the
        // credit gate admits any balance for a self-managed fleet at issue,
        // which is correct — there is nothing yet to charge them for.
        let rates = Posture::SelfManaged
            .rates_without_catalogue()
            .expect("self-managed prices without a catalogue");
        assert_eq!(slice_charge(rates, 0, 100, 0, 100), Nanos::ZERO);
        // And the run fee is what it will charge once time passes.
        assert_eq!(
            slice_charge(rates, 10_000, 0, 0, 0),
            Nanos::from_i64(10 * RUN_NANOS_PER_SEC)
        );
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
