//! A metering pass's lines priced, and the settlement they ride in.
//!
//! A renewal and a report each measure the lease's run time under its row lock
//! (a renewal in [`super::sql::renew::RENEW_BATCH`], or in
//! [`super::sql::meter::METER_LEASE`] on the per-lease path; a report inside its
//! own statement) and carry the runner's lines, one per provider request since
//! the last acknowledged renewal. This module prices
//! every request line by its own whole-prompt tier and its own request time,
//! adds one runtime-only line for the time the daemon measured, and wraps them
//! as the [`Settlement`] the lease's transaction logs and the billing relay
//! settles (billing §11).

mod original;
mod slice;

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_wire::usage::{RUNTIME_MS_MAX, RequestUsage, UsageLine};
use afd_billing::Nanos;
use afd_billing::biller::{Account, Attribution, Settlement, WorkKey};
use afd_billing::price::{PriceBook, Rater, Window};
use afd_billing::rates::Posture;
use afd_db::Db;
use jiff::Timestamp;

use crate::error::{Result, posture_corrupt, sequence_corrupt};

pub(crate) use self::slice::{Slice, SliceColumns, log_measured};

/// Logged when a pass could not read its prices and logged its tokens
/// unpriced.
const EVENT_TOKENS_UNPRICED: &str = "lease_tokens_unpriced";

/// What one lease's lines are priced against.
///
/// Read BEFORE the lease's transaction opens, so no catalogue read waits while
/// the lease and its slot are locked.
#[derive(Debug)]
pub struct Pricing {
    book: PriceBook,
    posture: Posture,
    provider: String,
    model: String,
    /// The catalogue version a cache looked this book up at, which a book
    /// with no price rows cannot name itself.
    looked_up_at: Option<i64>,
}

impl Pricing {
    /// The prices `provider`'s `model` is published at, for a run under
    /// `posture`.
    ///
    /// Fails OPEN: a catalogue that will not answer must not kill a run in
    /// flight. The pass then charges its runtime and logs its tokens unpriced:
    /// counted on the line, visible on the charges page, never silently free.
    pub async fn load(
        database: &Db,
        posture: Posture,
        provider: &str,
        model: &str,
        lease_id: &str,
    ) -> Self {
        let book = Self::read_book(database, posture, provider, model)
            .await
            .unwrap_or_else(|fault| {
            let Logged { error_code, reason } = fault.logged();
            tracing::warn!(
                error_code,
                lease_id,
                reason,
                event = EVENT_TOKENS_UNPRICED,
                "the catalogue could not be read; the run fee is charged and the tokens are logged unpriced"
            );
            PriceBook::default()
        });
        Self::of(book, posture, provider, model)
    }

    /// The pricing for `posture`, `provider` and `model`, or the fault that
    /// kept its book from being read, for a cache that must not keep a failure.
    ///
    /// # Errors
    /// A datastore that would not answer, or a catalogue row it cannot read.
    pub(crate) async fn try_load(
        database: &Db,
        posture: Posture,
        provider: &str,
        model: &str,
    ) -> Result<Self> {
        let book = Self::read_book(database, posture, provider, model).await?;
        Ok(Self::of(book, posture, provider, model))
    }

    async fn read_book(
        database: &Db,
        posture: Posture,
        provider: &str,
        model: &str,
    ) -> Result<PriceBook> {
        if posture == Posture::SelfManaged {
            return Ok(PriceBook::default());
        }
        let mut connection = database.acquire().await?;
        PriceBook::load(&mut *connection, provider, model)
            .await
            .map_err(crate::Error::from)
    }

    fn of(book: PriceBook, posture: Posture, provider: &str, model: &str) -> Self {
        Self {
            book,
            posture,
            provider: provider.to_owned(),
            model: model.to_owned(),
            looked_up_at: None,
        }
    }

    /// The lease's posture as its row stores it.
    ///
    /// # Errors
    /// The stored spelling is none this daemon writes, so the row was edited out
    /// of band. The lease is not priced: guessing the platform's posture would
    /// bill a tenant's own key at the platform's rates.
    pub(crate) fn posture_of(stored: &str) -> Result<Posture> {
        Posture::parse(stored).ok_or_else(posture_corrupt)
    }

    /// This pricing as a cache keeps it, looked up at `catalogue_version`.
    pub(crate) const fn looked_up_at(mut self, catalogue_version: Option<i64>) -> Self {
        self.looked_up_at = catalogue_version;
        self
    }

    /// The book these lines are priced from.
    pub(crate) const fn book(&self) -> &PriceBook {
        &self.book
    }

    /// The posture these lines are priced under.
    pub(crate) const fn posture(&self) -> Posture {
        self.posture
    }

    /// The provider these lines are priced against.
    pub(crate) fn provider(&self) -> &str {
        &self.provider
    }

    /// The model these lines are priced against.
    pub(crate) fn model(&self) -> &str {
        &self.model
    }

    /// The catalogue version a statement priced from this book guards on:
    /// only a platform run's tokens are priced from the catalogue.
    ///
    /// A book with no price rows names no version of its own, and a statement
    /// with no version skips its guard: a price added later would never be
    /// charged on this replica. A cached book therefore names the version it
    /// was looked up at.
    pub(crate) const fn catalogue_version(&self) -> Option<i64> {
        match self.posture {
            Posture::Platform => match self.book.catalogue_version() {
                Some(version) => Some(version),
                None => self.looked_up_at,
            },
            Posture::SelfManaged => None,
        }
    }

    /// The runner's request lines priced for a report whose window runs from
    /// the lease's start to `until`, without the runtime line the report's
    /// statement measures and prices itself.
    ///
    /// # Errors
    /// A charge that does not fit in a signed 64-bit nano count.
    pub(crate) fn request_lines(
        &self,
        from: Timestamp,
        until: Timestamp,
        requests: &[RequestUsage],
    ) -> Result<Vec<afd_billing::price::RatedLine>> {
        let rater = Rater::new(&self.book, self.posture, Window { from, until });
        Ok(requests
            .iter()
            .map(|request| rater.rate(&request.line()))
            .collect::<afd_billing::Result<Vec<_>>>()?)
    }

    /// The tier a runtime line measured at `at` is priced at, as the rater
    /// prices a line with no tokens: the covering tier on the platform's key,
    /// none on a self-managed one.
    pub(crate) fn runtime_price(&self, at: Timestamp) -> Option<Uuid7> {
        match self.posture {
            Posture::Platform => self.book.quote(at, 0).map(|tier| tier.id.clone()),
            Posture::SelfManaged => None,
        }
    }

    /// Prices for a pass that logs run time only, as an expired lease's last
    /// slice does: run time is charged under both postures and reads no
    /// catalogue.
    pub(crate) fn runtime_only(posture: Posture, provider: String, model: String) -> Self {
        Self {
            book: PriceBook::default(),
            posture,
            provider,
            model,
            looked_up_at: None,
        }
    }

    /// `requests` and the measured `slice` priced as lines of `lease_id`,
    /// wrapped as the settlement its biller charges.
    ///
    /// One line per request, at its own tier and time, plus a runtime-only line
    /// numbered by its slice's start when the slice measured run time. `None`
    /// when there is neither: logging nothing spends no line number.
    ///
    /// # Errors
    /// A stored identifier is not one, or a charge does not fit in a signed
    /// 64-bit nano count.
    pub(crate) fn settlement(
        &self,
        lease_id: &Uuid7,
        slice: Slice,
        requests: &[RequestUsage],
    ) -> Result<Option<Settlement>> {
        // The longest runtime one line may claim, which the renewal and report
        // statements cap their own slices at, so the three charge alike.
        let runtime_ms = count(slice.runtime_ms).min(RUNTIME_MS_MAX);
        let runtime = (runtime_ms > 0)
            .then(|| -> Result<UsageLine> {
                let seq = u32::try_from(slice.runtime_seq).map_err(|_range| sequence_corrupt())?;
                Ok(UsageLine::runtime(seq, slice.checked_at, runtime_ms))
            })
            .transpose()?;
        let lines: Vec<UsageLine> = requests
            .iter()
            .map(|request| request.line())
            .chain(runtime)
            .collect();
        if lines.is_empty() {
            return Ok(None);
        }
        let window = Window {
            from: slice.started_at,
            until: slice.checked_at,
        };
        let rater = Rater::new(&self.book, self.posture, window);
        let priced = lines
            .iter()
            .map(|line| rater.rate(line))
            .collect::<afd_billing::Result<Vec<_>>>()?;
        Ok(Some(Settlement {
            work: WorkKey::FleetLease(lease_id.clone()),
            payer: Account {
                tenant: slice.tenant_id,
                workspace: slice.workspace_id,
            },
            attribution: Attribution::Fleet {
                fleet_name: slice
                    .fleet_name
                    .unwrap_or_else(|| slice.fleet_id.clone().into()),
                fleet: slice.fleet_id,
                event_id: slice.event_id,
            },
            provider: self.provider.clone(),
            model: self.model.clone(),
            posture: self.posture,
            lines: priced,
        }))
    }
}

/// What one settlement's lines cost, for the caller that reports a charge.
pub(crate) fn charged(settlement: Option<&Settlement>) -> Nanos {
    settlement.map_or(Nanos::ZERO, |settled| {
        settled
            .lines
            .iter()
            .fold(Nanos::ZERO, |sum, line| sum.saturating_add(line.charged))
    })
}

/// A non-negative count as the line type holds it, saturating rather than
/// wrapping: an elapsed span the lease's hard cap bounds, or a line count, so
/// the saturation never fires.
fn count(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
