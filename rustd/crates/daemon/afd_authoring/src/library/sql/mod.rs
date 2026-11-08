//! Native Library rows are scoped by the authenticated workspace owner.
//!
//! An entry's public id is its platform slug, or its own id when it has none.

/// A visible entry by public id, share-locked.
///
/// `$1` public id, `$2` owner tenant, `$3` the public visibility.
pub(super) const ENTRY: &str = include_str!("entry.sql");

/// An entry the owner holds, locked for a change.
///
/// `$1` public id, `$2` owner tenant, `$3` the tenant visibility.
pub(super) const OWNED: &str = include_str!("owned.sql");

/// `$1` entry, `$2` revision.
pub(super) const REVISION: &str = include_str!("revision.sql");

/// One page of visible cards, each joined to its head revision, newest first.
///
/// One statement, so a card deleted mid-read is absent from the page rather
/// than half-read, and a page costs one round trip at any size.
///
/// `$1` owner tenant, `$2` owned only, `$3` the public visibility, `$4` cursor
/// instant, `$5` cursor id, `$6` limit, `$7` the tenant visibility.
pub(super) const PAGE: &str = include_str!("page.sql");

/// One page of an entry's revision metadata, newest first.
///
/// `$1` entry, `$2` cursor instant, `$3` cursor id, `$4` limit.
pub(super) const HISTORY: &str = include_str!("history.sql");

/// The creator's draft holding a candidate.
///
/// `$1` candidate, `$2` workspace, `$3` creator.
pub(super) const SOURCE: &str = include_str!("source.sql");

/// `$1` id, `$2` tenant, `$3` visibility, `$4` name, `$5` description,
/// `$6` head revision, `$7` resource version.
pub(super) const INSERT: &str = include_str!("insert.sql");

/// A platform entry; a slug already held inserts nothing.
///
/// `$1` id, `$2` visibility, `$3` name, `$4` description, `$5` head revision,
/// `$6` resource version, `$7` slug.
pub(super) const INSERT_PLATFORM: &str = include_str!("insert_platform.sql");

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

/// Staged and published platform entries, newest first.
///
/// `$1` and `$2` the platform visibilities.
pub(super) const PLATFORM_LIST: &str = include_str!("platform_list.sql");

/// A platform entry by slug, locked.
///
/// `$1` slug, `$2` and `$3` the platform visibilities.
pub(super) const PLATFORM_LOCK: &str = include_str!("platform_lock.sql");

/// The slug of the platform entry a revision belongs to.
///
/// `$1` revision.
pub(super) const PLATFORM_RECEIPT: &str = include_str!("platform_receipt.sql");

/// A conditional metadata edit; an edit that changes nothing writes nothing.
///
/// `$1` entry, `$2` name, `$3` description, `$4` credential reasons,
/// `$5` visibility; NULL keeps a field.
pub(super) const CURATE: &str = include_str!("curate.sql");
