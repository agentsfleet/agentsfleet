//! Native Plugin rows are scoped by the authenticated workspace owner.

/// A visible entry by public id, share-locked.
///
/// `$1` entry `UUIDv7`, `$2` owner tenant.
pub(super) const ENTRY: &str = include_str!("entry.sql");

/// An entry the owner holds, locked for a change.
///
/// `$1` entry `UUIDv7`, `$2` owner tenant.
pub(super) const OWNED: &str = include_str!("owned.sql");

/// `$1` entry, `$2` revision.
pub(super) const REVISION: &str = include_str!("revision.sql");

/// One page of visible cards, each joined to its head revision, newest first.
///
/// One statement, so a card deleted mid-read is absent from the page rather
/// than half-read, and a page costs one round trip at any size.
///
/// `$1` owner tenant, `$2` cursor instant, `$3` cursor id, `$4` limit.
pub(super) const PAGE: &str = include_str!("page.sql");

/// One page of an entry's revision metadata, newest first.
///
/// `$1` entry, `$2` cursor instant, `$3` cursor id, `$4` limit.
pub(super) const HISTORY: &str = include_str!("history.sql");

/// The creator's draft holding a candidate.
///
/// `$1` candidate, `$2` workspace, `$3` creator.
pub(super) const SOURCE: &str = include_str!("source.sql");

/// `$1` id, `$2` tenant, `$3` name, `$4` description,
/// `$5` head revision, `$6` resource version.
pub(super) const INSERT: &str = include_str!("insert.sql");

/// `$1` id, `$2` entry, `$3` revision number, `$4` definition, `$5` digest,
/// `$6` policy semantics.
pub(super) const APPEND: &str = include_str!("append.sql");

/// `$1` entry, `$2` the new head revision.
pub(super) const ADVANCE: &str = include_str!("advance.sql");

/// Records the publication on the draft as the revision it published.
///
/// `$1` draft, `$2` revision.
pub(super) const RECEIPT: &str = include_str!("receipt.sql");

/// The public id of the entry a revision belongs to.
///
/// `$1` revision.
pub(super) const ACCEPTED: &str = include_str!("accepted.sql");

/// Deletes an entry; its revisions go with it, and fleets keep their copies.
///
/// `$1` entry.
pub(super) const REMOVE: &str = include_str!("remove.sql");
