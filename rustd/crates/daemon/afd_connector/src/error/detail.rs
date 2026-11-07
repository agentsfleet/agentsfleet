//! The sentence each failure tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! The shared sentences are `afd_core::error`'s, the same constants
//! `afd_cron::error::detail` re-exports. Two planes answering one incident
//! with different prose read as two different bugs to whoever is holding the
//! page.

/// The detail every plane answers a datastore it cannot reach with.
pub use afd_core::error::DETAIL_DATABASE_UNAVAILABLE as DATABASE_UNAVAILABLE;

/// The detail every plane answers a datastore that refused a query with.
pub use afd_core::error::DETAIL_DATABASE_ERROR as DATABASE_ERROR;

/// The detail every plane answers an operation that did not complete with.
pub use afd_core::error::DETAIL_OPERATION_FAILED as OPERATION_FAILED;

/// What a person is told when the provider could not be reached at all.
///
/// It says nothing about which leg failed, because the only thing the person
/// can act on is the same either way: nothing was connected, and trying again
/// shortly may work.
pub const VENDOR_UNREACHABLE: &str = "Token exchange did not complete in time";

/// What a person is told when the provider answered and refused.
///
/// Deliberately does not say WHICH way it failed — a spent code, a mismatched
/// redirect URI and a rotated client secret are the same sentence to whoever
/// pressed Connect, and the difference is in the operator's log rather than in
/// the answer.
pub const EXCHANGE_FAILED: &str = "Token exchange failed";

/// What a person is told when no single GitHub installation could be bound.
///
/// One sentence for none listed, several listed, a claim the token does not
/// open and an installation another workspace routes: every one of them is
/// answered by installing the App on the right account, or signing in as the
/// account that owns it, and connecting again — and which one it was is in
/// the operator's log rather than the answer.
pub const INSTALLATION_OWNERSHIP: &str = "GitHub installation ownership could not be verified";

/// What a caller is told when the vendor declined to list installations.
///
/// Names the LISTING, not the exchange: by the time this is raised the code
/// has been redeemed and a token is in hand, so telling a person their token
/// exchange failed sends them to a credential that is working.
pub const INSTALLATION_LISTING_FAILED: &str =
    "GitHub would not list the installations for this authorization";

/// What a caller is told when a Disconnect is refused because a model entry
/// still names the connection's credential.
///
/// Says what to do, because the refusal leaves the connection exactly as it
/// was: the entry goes first, then the Disconnect.
pub const GRANT_STILL_REFERENCED: &str =
    "A model entry still names this connection's credential; remove that entry, then disconnect";
