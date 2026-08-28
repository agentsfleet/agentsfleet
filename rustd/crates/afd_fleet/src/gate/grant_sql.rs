//! The statements over `core.integration_grants` this crate reads.
//!
//! Copied from `state/integration_grant_lookup.zig`, which is the only Zig
//! module that answers "may this fleet use this integration". Read-only on
//! both sides: the request/approve/revoke half is the tenant plane's, and a
//! second writer of a standing human decision is exactly what must not exist.

/// The `status` a grant must hold for a fleet to mint against it.
///
/// One of three — `pending`, `approved`, `revoked` — and the only one that
/// admits anything. The other two are not spelled here because nothing in this
/// crate needs to tell them apart: absent, pending and revoked are all "no",
/// and a reader that distinguished them would invite a caller to treat one of
/// them as a maybe.
pub const STATUS_APPROVED: &str = "approved";

/// Every integration `fleet_id` may mint against, in one read.
///
/// ONE batch read per lease rather than one per declared credential: a fleet
/// declaring six credentials would otherwise pay six round trips to answer a
/// question the whole set shares.
///
/// `$1` fleet, `$2` the approved status.
pub const SELECT_APPROVED_SERVICES: &str = "\
SELECT service FROM core.integration_grants
WHERE fleet_id = $1::uuid AND status = $2";
