//! The three value objects a run is bounded by: what it may spend, where it
//! may reach, and how much context it may assemble.
//!
//! Each is built from its deserialized schema in one `TryFrom`, so the type
//! that leaves this module is already checked and nothing downstream re-checks
//! it.

use afd_validate::finite;
use afd_validate::rules::NOT_FINITE;

use crate::config::raw;
use crate::error::{Error, ErrorKind, Result, missing};

/// The key naming a fleet's daily spend ceiling.
const DAILY_DOLLARS: &str = "daily_dollars";
/// The key naming its monthly ceiling.
const MONTHLY_DOLLARS: &str = "monthly_dollars";

/// Most a fleet may declare as a daily ceiling.
const MAX_DAILY_DOLLARS: f64 = 1_000.0;
/// Most a fleet may declare as a monthly ceiling.
const MAX_MONTHLY_DOLLARS: f64 = 10_000.0;

/// Why a ceiling was refused.
const REASON_NOT_FINITE: &str = "it is not a finite amount";
/// See [`REASON_NOT_FINITE`].
const REASON_NOT_POSITIVE: &str = "a ceiling must be greater than zero";
/// See [`REASON_NOT_FINITE`].
const REASON_ABOVE_CAP: &str = "it is above the cap";

/// A spend ceiling: finite, greater than zero, and within its cap.
///
/// Dollars because that is the unit the authored document and the tenant
/// ledger both use. Whether the LEDGER should hold integer minor units is a
/// real question and belongs to the billing path, which owns the arithmetic;
/// this type only bounds what an author may declare.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Dollars(f64);

impl Dollars {
    /// Proves `amount` against `cap`.
    ///
    /// # Errors
    /// [`Error::InvalidBudget`] naming `field` and the rule it broke.
    fn parse(field: &'static str, amount: f64, cap: f64) -> Result<Self> {
        garde::Unvalidated::new(Amount { dollars: amount })
            .validate_with(&cap)
            .map(|proved| Self(proved.dollars))
            .map_err(|report| {
                let reason = reason(&report);
                ErrorKind::InvalidBudget { field, reason }.into()
            })
    }

    /// The ceiling, in dollars.
    #[must_use]
    pub const fn dollars(self) -> f64 {
        self.0
    }
}

/// An authored ceiling with the rules it must meet; its cap is the context.
///
/// `finite` comes first, and it is not redundant: NaN answers FALSE to both
/// `<= 0.0` and `> cap`, so garde's range alone admits it. garde runs the custom
/// rules in the order written and then the range, so the first break in the
/// report is the reason, in the order the rules read: finite, positive, cap.
#[derive(Debug, garde::Validate)]
#[garde(context(f64 as cap))]
struct Amount {
    #[garde(custom(finite), custom(positive), range(max = *cap))]
    dollars: f64,
}

/// Refuses zero and below: a ceiling of nothing stops a fleet before it runs.
///
/// # Errors
/// [`REASON_NOT_POSITIVE`] for an amount that is not greater than zero.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "garde fixes the custom-rule signature at `fn(&T, &C) -> garde::Result`; a float taken by value is not callable from the attribute that runs it"
)]
fn positive<C: ?Sized>(amount: &f64, _context: &C) -> garde::Result {
    if *amount > 0.0 {
        Ok(())
    } else {
        Err(garde::Error::new(REASON_NOT_POSITIVE))
    }
}

/// The reason each custom rule's report earns. A break neither names is the
/// range's, which is the cap.
const REASONS: [(&str, &str); 2] = [
    (NOT_FINITE, REASON_NOT_FINITE),
    (REASON_NOT_POSITIVE, REASON_NOT_POSITIVE),
];

/// The reason the first broken rule earns, in the order garde ran them.
fn reason(report: &garde::Report) -> &'static str {
    report
        .iter()
        .next()
        .and_then(|(_path, broken)| {
            REASONS
                .iter()
                .find(|(message, _reason)| *message == broken.message())
        })
        .map_or(REASON_ABOVE_CAP, |&(_message, reason)| reason)
}

/// What a fleet may spend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    /// The daily ceiling. Required — a fleet with no ceiling cannot be stopped
    /// by spending.
    daily: Dollars,
    /// The monthly ceiling, when one is declared.
    monthly: Option<Dollars>,
}

impl Budget {
    /// The daily ceiling.
    #[must_use]
    pub const fn daily(self) -> Dollars {
        self.daily
    }

    /// The monthly ceiling, when one was declared.
    #[must_use]
    pub const fn monthly(self) -> Option<Dollars> {
        self.monthly
    }
}

impl TryFrom<raw::Budget> for Budget {
    type Error = Error;

    fn try_from(authored: raw::Budget) -> Result<Self> {
        Ok(Self {
            daily: authored
                .daily_dollars
                .ok_or_else(|| missing(DAILY_DOLLARS))
                .and_then(|amount| Dollars::parse(DAILY_DOLLARS, amount, MAX_DAILY_DOLLARS))?,
            monthly: authored
                .monthly_dollars
                .map(|amount| Dollars::parse(MONTHLY_DOLLARS, amount, MAX_MONTHLY_DOLLARS))
                .transpose()?,
        })
    }
}

/// Where a fleet may reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    /// Hosts it may reach at all.
    allow: Box<[Box<str>]>,
    /// Whether egress is read-only.
    read_only: bool,
    /// Paths that stay writable even under [`read_only`](Self::read_only).
    read_post_paths: Box<[Box<str>]>,
}

impl Network {
    /// Hosts this fleet may reach.
    #[must_use]
    pub fn allow(&self) -> &[Box<str>] {
        &self.allow
    }

    /// Whether egress is read-only.
    #[must_use]
    pub const fn read_only(&self) -> bool {
        self.read_only
    }

    /// Paths that stay writable under [`read_only`](Self::read_only).
    #[must_use]
    pub fn read_post_paths(&self) -> &[Box<str>] {
        &self.read_post_paths
    }
}

impl TryFrom<raw::Network> for Network {
    type Error = Error;

    fn try_from(authored: raw::Network) -> Result<Self> {
        // No bounds here: the schema declared them and garde already proved
        // them. What is left is ownership, which is a `map`.
        let entries = |items: Option<Vec<String>>| {
            items
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect()
        };

        Ok(Self {
            allow: entries(authored.allow),
            // Absent reads as false: the permissive value is the one an author
            // gets by not thinking about it, and it is what every existing
            // document was written against.
            read_only: authored.read_only.unwrap_or(false),
            read_post_paths: entries(authored.read_post_paths),
        })
    }
}

/// How much context a run may assemble.
///
/// Zero means "auto" throughout — the runner substitutes its own default. That
/// sentinel is the runner's existing contract rather than something introduced
/// here, which is why [`raw::Knob`] resolves the word `"auto"` to it at the
/// boundary instead of carrying an `Option` the runner would have to re-read.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ContextBudget {
    /// Ceiling on the assembled context, in tokens.
    pub context_cap_tokens: u32,
    /// How much of the window tool output may occupy.
    pub tool_window: u32,
    /// How often the run checkpoints its memory.
    pub memory_checkpoint_every: u32,
    /// The fraction of the window that triggers stage chunking.
    pub stage_chunk_threshold: f32,
}

impl TryFrom<raw::Context> for ContextBudget {
    type Error = Error;

    fn try_from(authored: raw::Context) -> Result<Self> {
        if let Some(unknown) = authored.extra.keys().next() {
            return Err(ErrorKind::UnknownRuntimeKey {
                field: unknown.as_str().into(),
            }
            .into());
        }

        Ok(Self {
            context_cap_tokens: authored.context_cap_tokens.map_or(0, raw::Knob::or_auto),
            tool_window: authored.tool_window.map_or(0, raw::Knob::or_auto),
            memory_checkpoint_every: authored
                .memory_checkpoint_every
                .map_or(0, raw::Knob::or_auto),
            stage_chunk_threshold: authored.stage_chunk_threshold.unwrap_or(0.0),
        })
    }
}

#[cfg(test)]
#[path = "policy/tests.rs"]
mod tests;
