//! Fleet lifecycle guards and native revision projection, always workspace-scoped.
//!
//! A fleet's setup lives on its head revision: the definition, its review and
//! the version a client tags are columns of `fleet.revisions`, never copies on
//! the fleet row.

/// The fleet row alone. Locked before [`LOCKED`] reads it, never in one join:
/// a joined `FOR UPDATE` that waited re-checks its join against the row a
/// concurrent Apply committed, and the new revision fails that re-check, so a
/// live fleet would read as missing.
///
/// `$1` fleet, `$2` workspace.
pub(super) const LOCK: &str = include_str!("lock.sql");

/// The locked fleet with its head revision, counters and open gates, read under
/// a fresh statement snapshot that already includes whichever writer held the
/// lock before us.
///
/// `$1` fleet, `$2` workspace.
pub(super) const LOCKED: &str = include_str!("locked.sql");

/// `$1` candidate, `$2` workspace, `$3` creator.
pub(super) const CANDIDATE_DRAFT: &str = include_str!("candidate_draft.sql");

/// A visible Library revision by public Library id, share-locking its entry.
///
/// `$1` public Library id, `$2` revision, `$3` owner tenant, `$4` the public
/// visibility.
pub(super) const LIBRARY: &str = include_str!("library.sql");

/// The fleet row, pointing at a head revision written in the same transaction.
///
/// `$1` id, `$2` workspace, `$3` tenant, `$4` name, `$5` status, `$6` head
/// revision, `$7` required tags, `$8` initial start, `$9` resource version.
pub(super) const INSERT: &str = include_str!("insert.sql");

/// One immutable revision with the review it was accepted under.
///
/// `$1` id, `$2` fleet, `$3` revision number, `$4` definition, `$5` digest,
/// `$6` policy semantics, `$7` runtime model, `$8` review digest, `$9` the
/// frozen review subject, `$10` the source candidate.
pub(super) const REVISION: &str = include_str!("revision.sql");

/// Moves the fleet onto a new head revision.
///
/// `$1` fleet, `$2` revision, `$3` status.
pub(super) const ADVANCE: &str = include_str!("advance.sql");

/// `$1` draft, `$2` fleet, `$3` the saving status.
pub(super) const LINK: &str = include_str!("link.sql");

/// `$1` fleet, `$2` workspace, `$3` the installing status.
pub(super) const STREAM_READY: &str = include_str!("stream_ready.sql");

/// Activates an installing fleet still on the revision that was checked.
///
/// `$1` fleet, `$2` workspace, `$3` the active status, `$4` the installing
/// status, `$5` the checked revision number.
pub(super) const ACTIVATE: &str = include_str!("activate.sql");

/// `$1` fleet, `$2` workspace, `$3` initial start.
pub(super) const INITIAL: &str = include_str!("initial.sql");

/// Closes the drafts saving into a fleet, starting their retention.
///
/// `$1` fleet, `$2` the closed status, `$3` retention, `$4` the saving status.
pub(super) const CLOSE_DRAFTS: &str = include_str!("close_drafts.sql");

/// Closes one draft, starting its retention.
///
/// `$1` draft, `$2` the closed status, `$3` retention.
pub(super) const CLOSE_DRAFT: &str = include_str!("close_draft.sql");

/// `$1` fleet, `$2` workspace, `$3` status, `$4` name.
pub(super) const UPDATE: &str = include_str!("update.sql");

/// Returns a killed fleet's saving drafts to editing.
///
/// `$1` fleet, `$2` the editing status, `$3` the saving status.
pub(super) const RESTORE: &str = include_str!("restore.sql");

/// The number of a revision this fleet accepted.
///
/// `$1` revision, `$2` fleet.
pub(super) const ACCEPTED: &str = include_str!("accepted.sql");

/// `$1` workspace.
pub(super) const TENANT: &str = include_str!("tenant.sql");

/// `$1` fleet, `$2` workspace, `$3` the installing status.
pub(super) const REMOVE: &str = include_str!("remove.sql");

/// The owner's wallet, locked ahead of activation like every money path.
///
/// `$1` tenant.
pub(super) const WALLET: &str = include_str!("wallet.sql");
