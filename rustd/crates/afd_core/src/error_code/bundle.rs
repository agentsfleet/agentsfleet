//! The codes a Fleet Bundle answers with, and the platform library it is
//! curated in: `UZ-BUNDLE-*` is the snapshot a runner materialises support
//! files from, and `UZ-CATALOG-*` is the library row that publishes it. Split
//! from `fleet.rs` at the length cap; `REGISTRY` keeps one order for both.

use super::ErrorCode;

/// Untrusted Fleet Bundle bytes failed validation.
///
/// The detail kept by the importing service identifies the violated bound
/// without exposing content.
pub const FLEET_BUNDLE_INVALID: ErrorCode = ErrorCode::declare("UZ-BUNDLE-001");

/// No Fleet Bundle snapshot is stored under the requested content hash.
///
/// Not an error the runner acts on by retrying. A bundle with no support files
/// stores no snapshot at all, so this is the ORDINARY answer for a skill-only
/// fleet: the runner proceeds with no support files rather than failing the
/// run. The same code answers a hash that names nothing, and the two are
/// deliberately indistinguishable — a runner holding a hash from its own lease
/// cannot tell them apart and does not need to, and distinguishing them would
/// make the endpoint an oracle for which snapshots exist.
pub const FLEET_BUNDLE_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-BUNDLE-002");

/// A bundle whose declared credentials this workspace does not all hold.
///
/// Raised BEFORE the fleet row is written, so a workspace short a credential
/// ends with no fleet rather than an installed one that cannot run. A 424
/// rather than a 400: the request is well formed and the workspace is not ready
/// for it, and the body names which credentials to add.
pub const FLEET_BUNDLE_SECRETS_MISSING: ErrorCode = ErrorCode::declare("UZ-BUNDLE-003");

/// An external Fleet Bundle source could not be fetched.
pub const FLEET_BUNDLE_FETCH_FAILED: ErrorCode = ErrorCode::declare("UZ-BUNDLE-004");

/// The Fleet Bundle snapshot store is unconfigured, or would not answer.
///
/// One code for both, because the runner acts identically on either: it is a
/// 503, the work is not refused, and the poll comes back. Which of the two it
/// was is an OPERATOR's question, and it is answered in the log beside the
/// request id — an unconfigured store names a knob nobody set, and a fetch
/// failure carries the store's own error as its source.
pub const FLEET_BUNDLE_STORAGE_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-BUNDLE-005");

/// A bundle names a credential that is not a storable vault key.
///
/// Split from [`FLEET_BUNDLE_INVALID`] for the same reason
/// [`SSE_STREAM_CAP`](crate::error_code::SSE_STREAM_CAP) is split from
/// [`API_BACKPRESSURE`](crate::error_code::API_BACKPRESSURE): the remedy differs.
/// That code's message names a missing `SKILL.md` or an oversized file, so an
/// author who wrote `my-credential` instead of `my_credential` was sent to
/// re-package a bundle that was never malformed. The rule broken is carried on
/// `afd_fleet_runtime::Error::InvalidCredentialRef`, which already names the
/// reference and the rule; this code is what lets that reach the author.
pub const FLEET_BUNDLE_CREDENTIAL_NAME_INVALID: ErrorCode = ErrorCode::declare("UZ-BUNDLE-006");

/// No platform Fleet-library entry has the supplied slug.
pub const CATALOG_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-CATALOG-001");

/// A row without fetched bundle content cannot be published.
pub const CATALOG_PUBLISH_WITHOUT_BUNDLE: ErrorCode = ErrorCode::declare("UZ-CATALOG-002");

/// A published row must be withdrawn before deletion.
pub const CATALOG_DELETE_PUBLISHED: ErrorCode = ErrorCode::declare("UZ-CATALOG-003");

/// A different source repository already owns the bundle's declared slug.
pub const CATALOG_ID_COLLISION: ErrorCode = ErrorCode::declare("UZ-CATALOG-004");

/// The optional `If-Match` value no longer names the editable row.
pub const CATALOG_ROW_STALE: ErrorCode = ErrorCode::declare("UZ-CATALOG-005");
