//! A metered slice as one priced usage line, and the settlement it rides in.
//!
//! A renewal and a report each measure one slice under the lease's row lock
//! ([`super::sql::meter::METER_LEASE`]). This module prices that slice through
//! the [`Rater`] every usage line is priced by and wraps it as the
//! [`Settlement`] the lease's transaction logs and its biller settles after
//! commit (billing §11).
//!
//! # One line per slice, at the base tier
//!
//! Until runners report each provider request, a slice sums every request the
//! run made since its last renewal. The sum's prompt size says nothing about
//! any one request's, so the slice prices at the base tier
//! ([`PriceBook::into_base_tiers`]): choosing a context tier from a sum would
//! overcharge.

use afd_billing::biller::{Account, Attribution, Settlement, WorkKey};
use afd_billing::price::{PriceBook, Rater, Window};
use afd_billing::rates::Posture;
use afd_billing::{Cumulative, Nanos};
use afd_core::error_code::{Coded as _, Logged};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_wire::usage::UsageLine;
use jiff::Timestamp;

use crate::error::{Result, row_malformed};

/// Logged when a slice could not read its prices and left its tokens for a
/// later slice to charge.
const EVENT_TOKENS_HELD: &str = "lease_tokens_held_for_pricing";

/// The table a malformed slice identifier is reported against.
const TABLE_LEASES: &str = "runner.leases";

/// What one lease's slices are priced against.
///
/// Read BEFORE the lease's transaction opens, so no catalogue read waits while
/// the lease and its slot are locked.
#[derive(Debug)]
pub struct Pricing {
    book: PriceBook,
    posture: Posture,
    provider: String,
    model: String,
    /// Whether the book was read. When it was not, the caller meters zero
    /// token counts so the slot cursor stays where the last priced slice left
    /// it, and a later priced slice charges these tokens.
    priced: bool,
}

impl Pricing {
    /// The prices `provider`'s `model` is published at, for a run under
    /// `posture`.
    ///
    /// Fails OPEN: a catalogue that will not answer must not kill a run in
    /// flight. The slice then charges its runtime alone and holds its tokens
    /// for a priced slice; see [`Self::counts`].
    pub async fn load(
        database: &Db,
        posture: Posture,
        provider: &str,
        model: &str,
        lease_id: &str,
    ) -> Self {
        let read = async {
            if posture == Posture::SelfManaged {
                return Ok(PriceBook::default());
            }
            let mut connection = database.acquire().await?;
            PriceBook::load(&mut *connection, provider, model)
                .await
                .map_err(crate::Error::from)
        }
        .await;
        let (book, priced) = match read {
            Ok(book) => (book.into_base_tiers(), true),
            Err(fault) => {
                let Logged { error_code, reason } = fault.logged();
                tracing::warn!(
                    error_code,
                    lease_id,
                    reason,
                    event = EVENT_TOKENS_HELD,
                    "the catalogue could not be read; the run fee is charged and the tokens wait for a priced slice"
                );
                (PriceBook::default(), false)
            }
        };
        Self {
            book,
            posture,
            provider: provider.to_owned(),
            model: model.to_owned(),
            priced,
        }
    }

    /// The counts the metering statement may advance its cursor to.
    ///
    /// The reported counts when the book was read; zero otherwise, which the
    /// statement's `GREATEST` cursor clamp turns into "measure no tokens and
    /// leave the cursor where it is".
    #[must_use]
    pub fn counts(&self, reported: Cumulative) -> Cumulative {
        if self.priced {
            reported
        } else {
            Cumulative::default()
        }
    }

    /// `slice` priced as one usage line of `lease_id`, wrapped as the
    /// settlement its biller charges.
    ///
    /// `None` for a slice with no runtime and no tokens: there is nothing to
    /// log, and logging it would spend a line number on nothing.
    ///
    /// # Errors
    /// A stored identifier is not one, or a charge does not fit in a signed
    /// 64-bit nano count.
    pub(crate) fn settlement(&self, lease_id: &Uuid7, slice: Slice) -> Result<Option<Settlement>> {
        let usage = UsageLine {
            seq: slice.seq,
            at: slice.checked_at,
            input_tokens: count(slice.input),
            cached_input_tokens: count(slice.cached),
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens: count(slice.output),
            runtime_ms: count(slice.runtime_ms),
        };
        if usage.runtime_ms == 0 && usage.whole_prompt() == 0 && usage.output_tokens == 0 {
            return Ok(None);
        }
        let window = Window {
            from: slice.started_at,
            until: slice.checked_at,
        };
        let rated = Rater::new(&self.book, self.posture, window).rate(&usage)?;
        let id = |text: &str, column: &'static str| {
            Uuid7::parse(text).map_err(row_malformed(TABLE_LEASES, column))
        };
        Ok(Some(Settlement {
            work: WorkKey::FleetLease(lease_id.clone()),
            payer: Account {
                tenant: id(&slice.tenant_id, "tenant_id")?,
                workspace: id(&slice.workspace_id, "workspace_id")?,
            },
            attribution: Attribution::Fleet {
                fleet: id(&slice.fleet_id, "fleet_id")?,
                fleet_name: slice.fleet_name.unwrap_or_else(|| slice.fleet_id.clone()),
                event_id: slice.event_id,
            },
            provider: self.provider.clone(),
            model: self.model.clone(),
            posture: self.posture,
            lines: vec![rated],
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

/// The slice one metering pass measured, as its trailing row carries it.
#[derive(Debug)]
pub(crate) struct Slice {
    /// The line number this slice takes.
    pub seq: u32,
    /// The metering instant, which is the line's `at`.
    pub checked_at: Timestamp,
    /// When the lease began, which opens the line's pricing window.
    pub started_at: Timestamp,
    /// Runtime since the slot's cursor, in milliseconds.
    pub runtime_ms: i64,
    /// Prompt tokens since the cursor.
    pub input: i64,
    /// Cache-read tokens since the cursor.
    pub cached: i64,
    /// Completion tokens since the cursor.
    pub output: i64,
    /// Who pays.
    pub tenant_id: String,
    /// Where the work ran.
    pub workspace_id: String,
    /// The fleet the line names.
    pub fleet_id: String,
    /// The fleet's name when the line was written; `None` once purged.
    pub fleet_name: Option<String>,
    /// The event the run answered.
    pub event_id: String,
}

/// The slice columns of [`super::sql::meter::METER_LEASE`]'s answer.
///
/// Every column is nullable because the row comes from a `LEFT JOIN` on the
/// fence: a pass that lost it measured nothing.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct SliceColumns {
    checked_at: Option<jiff_sqlx::Timestamp>,
    started_at: Option<jiff_sqlx::Timestamp>,
    d_ms: Option<i64>,
    d_in: Option<i64>,
    d_cached: Option<i64>,
    d_out: Option<i64>,
    tenant_id: Option<String>,
    workspace_id: Option<String>,
    fleet_id: Option<String>,
    event_id: Option<String>,
    fleet_name: Option<String>,
    logged_lines: Option<i64>,
}

impl SliceColumns {
    /// The measured slice, when the pass held the fence.
    pub(crate) fn into_slice(self) -> Option<Slice> {
        Some(Slice {
            seq: u32::try_from(self.logged_lines?).unwrap_or(u32::MAX),
            checked_at: self.checked_at?.to_jiff(),
            started_at: self.started_at?.to_jiff(),
            runtime_ms: self.d_ms?,
            input: self.d_in?,
            cached: self.d_cached?,
            output: self.d_out?,
            tenant_id: self.tenant_id?,
            workspace_id: self.workspace_id?,
            fleet_id: self.fleet_id?,
            fleet_name: self.fleet_name,
            event_id: self.event_id?,
        })
    }
}

/// A non-negative delta as the line type holds it, saturating rather than
/// wrapping: the deltas are differences of `u32` counts and an elapsed span
/// the lease's hard cap bounds, so the saturation never fires.
fn count(delta: i64) -> u32 {
    u32::try_from(delta.max(0)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use afd_billing::Cumulative;
    use afd_billing::rates::Posture;
    use afd_core::test_util::trace::Capture;
    use tracing::Level;

    use super::{EVENT_TOKENS_HELD, Pricing};
    use crate::lease::test_dead;

    /// One second of runtime, the slice the purged-fleet case measures.
    const ONE_SECOND_MS: i64 = 1_000;

    /// The counts a runner reported, one per token class.
    const REPORTED: Cumulative = Cumulative {
        input: 25_000,
        cached: 2_500,
        output: 4_000,
    };

    /// A catalogue that cannot be read holds the slice's tokens: the warning
    /// names the lease, and the counts the statement may advance to are zero,
    /// so the cursor stays where the last priced slice left it.
    #[tokio::test]
    async fn an_unreadable_catalogue_holds_the_tokens_for_a_priced_slice() {
        let log = Capture::install();
        let pricing = Pricing::load(
            &test_dead::database(),
            Posture::Platform,
            "anthropic",
            "claude-opus-5-5",
            "lease-fixture",
        )
        .await;

        assert_eq!(pricing.counts(REPORTED), Cumulative::default());
        let event = log.only(EVENT_TOKENS_HELD);
        assert_eq!(event.level, Level::WARN);
        assert_eq!(event.field("lease_id"), Some("lease-fixture"));
        assert!(event.field("error_code").is_some());
    }

    /// A slice whose fleet row is gone still logs its line, naming the fleet
    /// by its identifier: the line is money owed, and a missing name must
    /// never cost it. A reader tells the fallback from a captured name because
    /// it equals `fleet_id`.
    #[tokio::test]
    async fn a_slice_of_a_purged_fleet_names_it_by_its_identifier() {
        let pricing = Pricing::load(
            &test_dead::database(),
            Posture::SelfManaged,
            "anthropic",
            "claude-opus-5-5",
            "lease-fixture",
        )
        .await;
        let fleet = test_dead::id(1);
        let slice = super::Slice {
            seq: 0,
            checked_at: test_dead::AT,
            started_at: test_dead::AT,
            runtime_ms: ONE_SECOND_MS,
            input: 0,
            cached: 0,
            output: 0,
            tenant_id: test_dead::id(2).as_str().to_owned(),
            workspace_id: test_dead::id(3).as_str().to_owned(),
            fleet_id: fleet.as_str().to_owned(),
            fleet_name: None,
            event_id: "1767225600000-1".to_owned(),
        };
        let settlement = pricing
            .settlement(&test_dead::id(4), slice)
            .expect("a runtime slice prices")
            .expect("a slice with runtime logs a line");
        let afd_billing::biller::Attribution::Fleet { fleet_name, .. } = &settlement.attribution
        else {
            unreachable!("a lease's line is attributed to its fleet");
        };
        assert_eq!(fleet_name.as_str(), fleet.as_str());
        assert_eq!(
            super::charged(Some(&settlement)).as_i64(),
            afd_billing::RUN_NANOS_PER_SEC,
            "one second of runtime is one second of run fee"
        );
    }

    /// A slice that measured nothing logs nothing.
    #[tokio::test]
    async fn an_empty_slice_logs_no_line() {
        let pricing = Pricing::load(
            &test_dead::database(),
            Posture::SelfManaged,
            "anthropic",
            "claude-opus-5-5",
            "lease-fixture",
        )
        .await;
        let slice = super::Slice {
            seq: 0,
            checked_at: test_dead::AT,
            started_at: test_dead::AT,
            runtime_ms: 0,
            input: 0,
            cached: 0,
            output: 0,
            tenant_id: test_dead::id(2).as_str().to_owned(),
            workspace_id: test_dead::id(3).as_str().to_owned(),
            fleet_id: test_dead::id(1).as_str().to_owned(),
            fleet_name: None,
            event_id: "1767225600000-1".to_owned(),
        };
        assert_eq!(
            pricing
                .settlement(&test_dead::id(4), slice)
                .expect("prices"),
            None
        );
    }

    /// A self-managed lease reads no catalogue, so a dead one changes nothing:
    /// the reported counts pass through and nothing is logged.
    #[tokio::test]
    async fn a_self_managed_lease_prices_without_the_catalogue() {
        let log = Capture::install();
        let pricing = Pricing::load(
            &test_dead::database(),
            Posture::SelfManaged,
            "anthropic",
            "claude-opus-5-5",
            "lease-fixture",
        )
        .await;

        assert_eq!(pricing.counts(REPORTED), REPORTED);
        assert!(
            log.events()
                .iter()
                .all(|event| event.field("event") != Some(EVENT_TOKENS_HELD)),
            "a self-managed lease never reads the catalogue, so nothing is held"
        );
    }
}
