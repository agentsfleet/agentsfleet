//! What a schedule IS, as three closed vocabularies and one row.
//!
//! # Why these are enums and not the strings the column holds
//!
//! `520_fleet_schedules.sql` stores `source`, `desired_status` and
//! `sync_status` as `TEXT` with no `CHECK`, deliberately — RULE STS bans a
//! frozen vocabulary in the schema, because changing one then means a migration
//! rather than a release. The vocabulary still has to be closed SOMEWHERE, and
//! this is where: a value the column holds that no variant names is a row this
//! build cannot read, reported as such rather than defaulted past.
//!
//! Defaulting would be the real failure. A `desired_status` written by a newer
//! daemon and read here as "active" would push a schedule the operator had
//! deleted back to the external scheduler on the next sync.

use afd_core::id::Uuid7;

/// The most schedules one fleet may hold.
///
/// `model.zig`'s `MAX_SCHEDULES_PER_FLEET`. A bound on the fan-out of one
/// fleet's cron traffic, and the reason a create is refused rather than queued:
/// an author who has hit it has a configuration problem a silent acceptance
/// would hide until the invoice.
pub const MAX_SCHEDULES_PER_FLEET: usize = 32;

/// The timezone a schedule that named none is interpreted in.
pub const DEFAULT_TIMEZONE: &str = "UTC";

/// Who asked for this schedule to exist.
///
/// The distinction is ownership, not provenance: a `Trigger` schedule is
/// derived from the fleet's stored document and is rewritten whenever that
/// document is, while an `Api` schedule was created by a person and survives a
/// document edit. A sync that could not tell them apart would delete one of
/// them on every install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Created through the schedules surface by a person.
    Api,
    /// Derived from a `cron` trigger in the fleet's stored document.
    Trigger,
}

impl Source {
    /// Every source, for the readers that walk them.
    pub const ALL: &'static [Self] = &[Self::Api, Self::Trigger];

    /// The word the column holds.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Trigger => "trigger",
        }
    }

    /// The source a stored word names, when this build knows one.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == stored)
    }
}

/// What the operator wants this schedule to be doing.
///
/// The INTENT half of the pair. Separate from [`SyncStatus`] because they
/// answer different questions and can disagree for a while by design: a
/// schedule an operator paused is `Paused` the moment they say so, and stays
/// `Syncing` until the external scheduler has been told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesiredStatus {
    /// Should fire on its expression.
    Active,
    /// Should exist and not fire.
    Paused,
    /// Should cease to exist once the external scheduler agrees.
    ///
    /// A state and not a deletion, because the row cannot go until the upstream
    /// schedule has: deleting it first would leave a schedule firing at a
    /// callback this daemon can no longer resolve to a fleet.
    Deleting,
}

impl DesiredStatus {
    /// Every intent, for the readers that walk them.
    pub const ALL: &'static [Self] = &[Self::Active, Self::Paused, Self::Deleting];

    /// The word the column holds.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Deleting => "deleting",
        }
    }

    /// The intent a stored word names, when this build knows one.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == stored)
    }

    /// Whether a fire arriving for this schedule should wake the fleet.
    ///
    /// Only `Active` does. A `Paused` schedule that fires is the external
    /// scheduler not yet knowing, and a `Deleting` one is the same — both are
    /// dropped rather than refused, because the sender is a correctly
    /// configured provider acting on what it was last told.
    #[must_use]
    pub const fn fires(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// How far the external scheduler has been brought in line with the intent.
///
/// The OBSERVED half of the pair — see [`DesiredStatus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    /// A syncer holds this row and the upstream call is outstanding.
    Syncing,
    /// The external scheduler agrees with the intent.
    Synced,
    /// The last attempt failed; `last_error` says how.
    ///
    /// Not terminal. `:sync` retries it, which is the whole reason the failure
    /// is a stored state rather than a discarded one.
    Failed,
}

impl SyncStatus {
    /// Every sync state, for the readers that walk them.
    pub const ALL: &'static [Self] = &[Self::Syncing, Self::Synced, Self::Failed];

    /// The word the column holds.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Syncing => "syncing",
            Self::Synced => "synced",
            Self::Failed => "failed",
        }
    }

    /// The sync state a stored word names, when this build knows one.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == stored)
    }
}

/// One schedule, as the row holds it.
///
/// # Why `generation`, `sync_token` and `sync_lease_until` are on the value
///
/// They are not decoration on the row, they are the fence. `generation` is the
/// optimistic-concurrency counter a finalize compares against, and the token
/// plus the lease are what stop two syncers pushing the same schedule at once.
/// A value type that dropped them would let a caller finalize a claim it no
/// longer holds — which is the one failure this table's design exists to
/// prevent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    /// This schedule's own identity, distinct from its upstream key.
    pub schedule_id: Uuid7,
    /// The fleet a fire wakes.
    pub fleet_id: Uuid7,
    /// Who asked for it.
    pub source: Source,
    /// The key the external scheduler knows it by.
    ///
    /// Unique per fleet, and deliberately a different value from
    /// [`Self::schedule_id`]: a schedule re-registered upstream gets a new key
    /// while staying the same schedule to everyone here.
    pub source_key: String,
    /// The expression, in the grammar `validate` accepts.
    pub cron: String,
    /// The zone the expression is interpreted in.
    pub timezone: String,
    /// What the fleet is asked to do when it fires.
    pub message: String,
    /// What the operator wants.
    pub desired_status: DesiredStatus,
    /// How far upstream has been brought in line.
    pub sync_status: SyncStatus,
    /// The optimistic-concurrency counter, always above zero.
    pub generation: i64,
    /// The syncer holding this row, when one does.
    pub sync_token: Option<String>,
    /// When that hold expires, so a syncer that died does not hold forever.
    pub sync_lease_until: Option<i64>,
    /// How the last attempt failed, when one did.
    pub last_error: Option<String>,
    /// When the row was written.
    pub created_at: i64,
    /// When it was last changed.
    pub updated_at: i64,
}
