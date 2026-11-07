//! One usage line priced: [`Rater::rate`].

use afd_core::id::Uuid7;
use afd_wire::usage::UsageLine;
use jiff::Timestamp;

use super::PriceBook;
use crate::error::charge_overflow;
use crate::nanos::{MS_PER_SEC, TOKENS_PER_MTOK};
use crate::{Nanos, Posture, RUN_NANOS_PER_SEC, Result};

/// The span a piece of work ran over, which bounds a line's `at`.
///
/// A runner reports when each request was made; the daemon clamps that to the
/// work's own window before choosing a price, so a reported time can neither
/// backdate a request into a cheaper range nor push it past the work's end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// When the work started.
    pub from: Timestamp,
    /// The latest instant a line of this work may claim.
    pub until: Timestamp,
}

impl Window {
    /// `at`, moved inside the window if it falls outside.
    #[must_use]
    pub fn clamp(self, at: Timestamp) -> Timestamp {
        at.clamp(self.from, self.until.max(self.from))
    }
}

/// A usage line with its price: what every biller receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatedLine {
    /// The line as reported, with `at` clamped to its work's window.
    pub usage: UsageLine,
    /// The catalogue tier it was priced at; `None` when no tier covered it or
    /// the work is self-managed.
    pub price: Option<Uuid7>,
    /// What the line costs: runtime plus every priced token category.
    pub charged: Nanos,
    /// Tokens in categories the chosen tier leaves unknown, or every token when
    /// no tier covered the request. Visible, never silently free.
    pub unpriced_tokens: u64,
}

/// Prices one piece of work's lines against one model's [`PriceBook`].
#[derive(Debug, Clone, Copy)]
pub struct Rater<'a> {
    book: &'a PriceBook,
    posture: Posture,
    window: Window,
}

impl<'a> Rater<'a> {
    /// A rater for work run under `posture` inside `window`.
    #[must_use]
    pub const fn new(book: &'a PriceBook, posture: Posture, window: Window) -> Self {
        Self {
            book,
            posture,
            window,
        }
    }

    /// `line`, priced.
    ///
    /// Runtime is charged under both postures; tokens only on the platform's
    /// key. The tier is chosen by the request's whole prompt and its clamped
    /// time. Every category rounds UP to a whole nano on its own, so the
    /// platform never under-bills and over-bills by at most one nano per
    /// category per line (Indy, 2026-10-19).
    ///
    /// # Errors
    /// A charge that does not fit in a signed 64-bit nano count.
    pub fn rate(&self, line: &UsageLine) -> Result<RatedLine> {
        let mut usage = *line;
        usage.at = self.window.clamp(line.at);
        let runtime = ceil_div(
            i128::from(usage.runtime_ms) * i128::from(RUN_NANOS_PER_SEC),
            i128::from(MS_PER_SEC),
        );

        let (price, tokens, unpriced_tokens) = match self.posture {
            Posture::SelfManaged => (None, 0, 0),
            Posture::Platform => match self.book.quote(usage.at, usage.whole_prompt()) {
                None => (None, 0, tokens_of(&usage)),
                Some(tier) => {
                    let (cost, unpriced) = token_cost(&usage, &tier.price);
                    (Some(tier.id.clone()), cost, unpriced)
                }
            },
        };

        let charged = i64::try_from(runtime + tokens).map_err(|_overflow| charge_overflow())?;
        Ok(RatedLine {
            usage,
            price,
            charged: Nanos::from_i64(charged),
            unpriced_tokens,
        })
    }
}

/// Every token a line reports, in any category.
fn tokens_of(usage: &UsageLine) -> u64 {
    usage.whole_prompt() + u64::from(usage.output_tokens)
}

/// The token cost of `usage` at `price`, and the tokens left unpriced.
///
/// `i128` throughout: a `u32` count times a 64-bit rate cannot overflow it, so
/// the only fallible step is the final narrowing in [`Rater::rate`].
fn token_cost(usage: &UsageLine, price: &afd_core::catalogue::Price) -> (i128, u64) {
    let categories = [
        (usage.input_tokens, Some(price.input_nanos_per_mtok)),
        (
            usage.cached_input_tokens,
            Some(price.cached_input_nanos_per_mtok),
        ),
        (usage.cache_write_tokens, price.cache_write_nanos_per_mtok),
        (
            usage.cache_write_5m_tokens,
            price.cache_write_5m_nanos_per_mtok,
        ),
        (
            usage.cache_write_1h_tokens,
            price.cache_write_1h_nanos_per_mtok,
        ),
        (usage.output_tokens, Some(price.output_nanos_per_mtok)),
    ];
    categories
        .into_iter()
        .fold((0, 0), |(cost, unpriced), (tokens, rate)| match rate {
            Some(rate) => (
                cost + ceil_div(
                    i128::from(tokens) * i128::from(rate),
                    i128::from(TOKENS_PER_MTOK),
                ),
                unpriced,
            ),
            None => (cost, unpriced + u64::from(tokens)),
        })
}

/// `numerator / denominator`, rounded up, for a non-negative numerator.
const fn ceil_div(numerator: i128, denominator: i128) -> i128 {
    (numerator + denominator - 1) / denominator
}
