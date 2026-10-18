//! The one billing lifecycle every product path runs through.
//!
//! Work places a hold before it runs ([`Biller::reserve`]), charges its priced
//! lines as it goes ([`Biller::settle`]), and frees what is left when it ends
//! ([`Biller::release`]). The built-in implementation is
//! [`LedgerBiller`](crate::ledger::LedgerBiller); a queue or a third-party
//! metering system is another implementation of the same trait, held as
//! `Arc<dyn Biller>` (billing §11).
//!
//! The work's own transaction records what happened in `billing.usage_lines`
//! (always ours); a biller moves money after commit. Every method is
//! idempotent on its key, so a retry or the sweeper never charges twice.

use std::time::Duration;

use afd_core::id::Uuid7;
use jiff::Timestamp;

use crate::price::RatedLine;
use crate::{Nanos, Posture, Result, sql};

pub(crate) mod observed;
mod outage;

pub use self::observed::Observed;
pub use self::outage::{OutagePolicy, WithOutagePolicy};

#[cfg(feature = "test-util")]
pub mod conformance;
#[cfg(feature = "test-util")]
mod recording;
#[cfg(feature = "test-util")]
pub use self::recording::RecordingBiller;

/// The piece of work money is held and charged for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WorkKey {
    /// A fleet run's lease.
    FleetLease(Uuid7),
    /// A setup-chat attempt.
    AuthoringAttempt(Uuid7),
}

impl WorkKey {
    /// The stored spelling of this work's kind (`billing.*.work_type`).
    #[must_use]
    pub const fn work_type(&self) -> &'static str {
        match self {
            Self::FleetLease(_) => sql::work::FLEET_LEASE,
            Self::AuthoringAttempt(_) => sql::work::AUTHORING_ATTEMPT,
        }
    }

    /// The work's own identifier.
    #[must_use]
    pub const fn id(&self) -> &Uuid7 {
        match self {
            Self::FleetLease(id) | Self::AuthoringAttempt(id) => id,
        }
    }
}

/// Who pays, and where the work ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The paying tenant: the workspace owner's account.
    pub tenant: Uuid7,
    /// The workspace the work ran in.
    pub workspace: Uuid7,
}

/// What a charge names beside its work, for the charges page and budgets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribution {
    /// A fleet run: the fleet, its name at charge time, and the event it ran.
    Fleet {
        /// The fleet.
        fleet: Uuid7,
        /// The fleet's name when the line was written.
        fleet_name: String,
        /// The event the run answered.
        event_id: String,
    },
    /// A setup-chat attempt; the attempt is the work itself.
    SetupChat,
}

/// A request to hold up to `ceiling` for `work` against `payer`'s balance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservation {
    /// The work the hold is for.
    pub work: WorkKey,
    /// Who pays.
    pub payer: Account,
    /// The most the work may spend in total, captured lines included.
    pub ceiling: Nanos,
}

/// A biller's answer to a [`Reservation`].
///
/// An ordinary answer rather than an error: a short balance is a decision the
/// caller acts on, not a failure (FN-RS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// The work may spend up to `ceiling`; `captured` is already charged.
    Held {
        /// The hold's ceiling after this reservation.
        ceiling: Nanos,
        /// What the work's settled lines have taken so far.
        captured: Nanos,
    },
    /// The balance cannot cover the ask.
    Short {
        /// What the payer could still hold, other holds deducted.
        available: Nanos,
        /// The ceiling that was asked for.
        asked: Nanos,
    },
    /// The work's hold was already released; it may not reserve again.
    Closed,
    /// The biller could not answer and the outage policy admitted the work.
    Unconfirmed,
}

impl Hold {
    /// Whether the work may run.
    #[must_use]
    pub const fn admits(self) -> bool {
        matches!(self, Self::Held { .. } | Self::Unconfirmed)
    }
}

/// Priced lines of one piece of work, ready to charge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    /// The work the lines belong to.
    pub work: WorkKey,
    /// Who pays.
    pub payer: Account,
    /// What the charge names.
    pub attribution: Attribution,
    /// The model's provider key.
    pub provider: String,
    /// The provider's model identifier.
    pub model: String,
    /// Whose key paid for the tokens.
    pub posture: Posture,
    /// Each line's usage and its price.
    pub lines: Vec<RatedLine>,
}

/// What one [`Biller::settle`] charged.
///
/// `charged` is what the newly settled lines cost; `taken` is what the balance
/// could give. A balance that cannot cover its lines is clamped at zero, and
/// the difference is `written_off`, never silent: the biller logs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Receipt {
    /// Lines this call charged; lines charged before are not counted again.
    pub settled_lines: usize,
    /// What the newly settled lines cost.
    pub charged: Nanos,
    /// What this call actually took from the balance.
    pub taken: Nanos,
    /// What the balance could not cover: `charged` less `taken`.
    pub written_off: Nanos,
}

/// Why a piece of work's hold is released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Release {
    /// The work finished and reported.
    Completed,
    /// The work failed.
    Failed,
    /// The work's lease expired unreported. The caller logs and settles the
    /// runtime-only line for its last `runtime` before releasing; a biller
    /// that keeps no line log of its own may read it here.
    Expired {
        /// Run time since the last settled line.
        runtime: Duration,
    },
}

/// A payer's balance as a biller sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Balance {
    /// Spendable credit.
    pub balance: Nanos,
    /// Credit held by open reservations and not yet charged.
    pub held: Nanos,
    /// When the balance last reached zero.
    pub exhausted_at: Option<Timestamp>,
}

/// Holds, charges and releases money for work (billing §11).
///
/// Every method is idempotent on its key: `reserve` and `release` on the work,
/// `settle` on each line's `(work, seq)`.
#[async_trait::async_trait]
pub trait Biller: Send + Sync + std::fmt::Debug {
    /// Holds up to `ask.ceiling` for the work, or answers why not. Raising an
    /// existing hold is the same call with a higher ceiling.
    ///
    /// # Errors
    /// The biller could not answer. [`WithOutagePolicy`] decides what the work
    /// does then.
    async fn reserve(&self, ask: &Reservation) -> Result<Hold>;

    /// Charges `settlement`'s lines that were not charged before.
    ///
    /// # Errors
    /// The biller could not answer; the lines stay unsettled for the sweeper.
    async fn settle(&self, settlement: &Settlement) -> Result<Receipt>;

    /// Charges anything still unsettled for `work`, then frees its hold.
    ///
    /// # Errors
    /// The biller could not answer; the sweeper retries.
    async fn release(&self, work: &WorkKey, why: Release) -> Result<()>;

    /// `payer`'s balance and what is held against it.
    ///
    /// # Errors
    /// The biller could not answer, or the payer has no balance at all.
    async fn balance(&self, payer: &Account) -> Result<Balance>;
}
