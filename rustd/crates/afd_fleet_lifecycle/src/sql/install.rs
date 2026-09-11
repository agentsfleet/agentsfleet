//! The Fleet-library rows an install resolves a bundle from.
//!
//! Separate from [`super`] because these read `core.fleet_library`, not
//! `core.fleets`: a different table, a different tenancy predicate, and a
//! different reason to change.

/// A platform library entry, resolved for install by its slug.
///
/// `$1` the entry's id · `$2` the visibility a published row carries.
///
/// Only a PUBLISHED row holding a bundle is installable. A draft resolves
/// nothing, so an unpublished fleet cannot be installed by anybody who merely
/// knows its identifier — the predicate is the check, rather than a handler
/// remembering to make one.
pub(crate) const SELECT_PLATFORM_INSTALL: &str = "\
SELECT skill_markdown, trigger_markdown, content_hash \
FROM core.fleet_library \
WHERE id = $1 AND visibility = $2 \
  AND content_hash IS NOT NULL AND skill_markdown IS NOT NULL";

/// A tenant library entry, resolved for install and scoped to its workspace.
///
/// `$1` the entry's id · `$2` workspace. An entry another workspace owns is
/// invisible here rather than forbidden, for the reason every statement in this
/// file scopes: a refusal that told the two apart would disclose the entry.
pub(crate) const SELECT_TENANT_INSTALL: &str = "\
SELECT skill_markdown, trigger_markdown, content_hash \
FROM core.tenant_fleet_library \
WHERE id = $1::uuid AND workspace_id = $2::uuid";
