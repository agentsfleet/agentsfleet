//! The codes that answer for a third party this daemon reaches on a fleet's
//! behalf — the credential it brokers, and the provider it spends that
//! credential at.
//!
//! An integration that is not connected, a broker nobody configured, a GitHub
//! App that needs reconnecting, a grant that is not there, an OAuth exchange
//! that failed, and the three refusals a repository-write mint can carry.
//!
//! `UZ-PROVIDER-*` is the other half of the same boundary: WHICH third party a
//! run may dial and at what price. Four of the five are about the priced
//! catalogue row naming that provider — a row this daemon owns — and they sit
//! here rather than with the request codes because the row exists only to
//! describe an outside vendor, so an operator reading a catalogue refusal is
//! already looking at the integration plane.

use super::ErrorCode;

/// The workspace has connected no integration under the requested name.
///
/// `ERR_CRED_INTEGRATION_NOT_CONNECTED` (`error_registry.zig:228`). A 404, and
/// the answer for BOTH "the vault holds no handle" and "the handle names a
/// connector this daemon does not carry" — a runner acts identically on either,
/// and telling them apart would make the mint an oracle for which connectors a
/// deployment ships.
pub const CRED_INTEGRATION_NOT_CONNECTED: ErrorCode = ErrorCode::declare("UZ-CRED-001");

/// This deployment has no credential broker wired.
///
/// `ERR_CRED_BROKER_NOT_CONFIGURED` (`error_registry.zig:229`). A 503, and an
/// OPERATOR's fault rather than a tenant's: the broker is a boot-wired
/// singleton, so its absence is a deployment that was never set up to mint.
pub const CRED_BROKER_NOT_CONFIGURED: ErrorCode = ErrorCode::declare("UZ-CRED-002");

/// The selected provider/model pair has no priced catalogue row.
pub const PROVIDER_MODEL_NOT_IN_CATALOGUE: ErrorCode = ErrorCode::declare("UZ-PROVIDER-004");

/// A custom endpoint is missing, forbidden, or unsafe for its provider.
pub const PROVIDER_BASE_URL_INVALID: ErrorCode = ErrorCode::declare("UZ-PROVIDER-005");

/// No priced catalogue row matches the operator-supplied identifier.
pub const PROVIDER_MODEL_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-PROVIDER-006");

/// The active platform default still references the catalogue row.
pub const PROVIDER_MODEL_IN_USE: ErrorCode = ErrorCode::declare("UZ-PROVIDER-007");

/// A priced row already has the requested provider/model identity.
pub const PROVIDER_MODEL_EXISTS: ErrorCode = ErrorCode::declare("UZ-PROVIDER-008");

/// The GitHub App installation is gone.
///
/// `ERR_GH_RECONNECT_REQUIRED` (`error_registry.zig:230`). Uninstalled or
/// revoked, so no token can be minted from it. A HUMAN's remedy — no amount of
/// retrying reconnects an App somebody removed — which is why it is its own
/// code rather than a mint failure.
pub const GH_RECONNECT_REQUIRED: ErrorCode = ErrorCode::declare("UZ-GH-001");

/// GitHub did not return an installation token this daemon would hand over.
///
/// `ERR_GH_MINT_FAILED` (`error_registry.zig:231`). One code for both retry
/// classes, deliberately: the runner reacts the same way to a vendor outage and
/// to a malformed exchange, and the class is the broker's own concern. It also
/// answers a token that came back reaching FURTHER than the fleet declared —
/// the exchange worked, the credential was discarded, and a runner cannot be
/// told the difference without being told what it nearly received.
pub const GH_MINT_FAILED: ErrorCode = ErrorCode::declare("UZ-GH-002");

/// The fleet holds no approved grant for the integration it asked to mint.
///
/// `ERR_GRANT_NOT_FOUND` (`error_registry.zig:195`). Absent, pending and
/// revoked all answer this: only an approved standing decision admits anything,
/// and a caller able to tell them apart would treat pending as a maybe.
pub const GRANT_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-GRANT-001");

/// The revoke addressed a grant this fleet does not hold.
///
/// `ERR_GRANT_REVOKE_NOT_FOUND` (`error_registry.zig:196`). A 404 where
/// [`GRANT_NOT_FOUND`] is a 403, and the difference is who is being refused:
/// that one tells a RUNNER its mint is ungranted, this tells an OPERATOR the
/// row they aimed at is not there. "Already revoked" answers the same code on
/// purpose — the revoke is idempotent in intent and the grant is not usable
/// either way, so telling the two apart would only invite a caller to retry.
pub const GRANT_REVOKE_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-GRANT-002");

/// A connector's OAuth exchange was rejected.
///
/// `ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED` (`error_registry.zig:239`). The
/// refresh-grant connectors' answer to BOTH a dead refresh token and a failed
/// exchange, where GitHub has two codes for the same pair. That asymmetry is
/// the Zig's and is kept: a Zoho failure must never tell a runner to reconnect
/// a GitHub App, and the shared connector code is what stops it.
pub const CONNECTOR_OAUTH_EXCHANGE_FAILED: ErrorCode = ErrorCode::declare("UZ-CONN-006");

/// No human approved a repository-write gate for this event.
///
/// `ERR_REPAIR_WRITE_UNAPPROVED` (`error_registry.zig:199`). The run is not
/// refused — it continues read-only — so this is a refusal of the TOKEN and not
/// of the work.
pub const REPAIR_WRITE_UNAPPROVED: ErrorCode = ErrorCode::declare("UZ-REPAIR-010");

/// The fleet's declared reach no longer matches the approved card.
///
/// `ERR_REPAIR_BINDING_DRIFT` (`error_registry.zig:200`). Its own code rather
/// than an unapproved gate, because the remedy differs: an approval exists and
/// a human must be shown the reach the fleet declares NOW.
pub const REPAIR_BINDING_DRIFT: ErrorCode = ErrorCode::declare("UZ-REPAIR-011");

/// The approved write allowance is spent.
///
/// `ERR_REPAIR_SPEND_EXHAUSTED` (`error_registry.zig:202`). The one refusal in
/// this group that says the approval was real and was HONOURED — as far as it
/// went.
pub const REPAIR_SPEND_EXHAUSTED: ErrorCode = ErrorCode::declare("UZ-REPAIR-013");
