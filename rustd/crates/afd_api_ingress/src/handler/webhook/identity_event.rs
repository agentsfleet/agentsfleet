//! The identity provider's `user.created` payload, as the signup route reads it.
//!
//! Split from `identity_route.rs` at its length cap: what the provider sends
//! and how an address and a name are read off it, apart from the route that
//! verifies the delivery and opens the account.

use afd_observability::metrics::label::fleet::SignupFailure;
use serde::Deserialize;

/// The identity provider's `user.created` payload, tolerant of unknown fields.
///
/// Unknown fields are ignored rather than refused, which is the port's rule and
/// not laxity: the provider adds fields to these payloads without notice, and a
/// daemon that refused an unrecognised one would go down on a vendor's release
/// note.
#[derive(Debug, Deserialize)]
pub(super) struct IdentityEvent {
    /// Which event this is.
    #[serde(rename = "type")]
    pub(super) kind: String,
    /// The person it is about.
    pub(super) data: IdentityUser,
}

/// The person an identity event describes.
#[derive(Debug, Deserialize)]
pub(super) struct IdentityUser {
    /// The provider's own subject, and the account's unique key.
    pub(super) id: String,
    /// Every address the provider holds for them.
    #[serde(default)]
    email_addresses: Vec<IdentityEmail>,
    /// Which of those is primary.
    #[serde(default)]
    primary_email_address_id: Option<String>,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    last_name: Option<String>,
}

/// One address the provider holds.
#[derive(Debug, Deserialize)]
struct IdentityEmail {
    /// Its own id, which `primary_email_address_id` names.
    id: String,
    /// The address itself.
    email_address: String,
    /// The provider's proof that the person reads it; absent reads as none.
    #[serde(default)]
    verification: Option<Verification>,
}

/// What the provider says it proved about one address.
#[derive(Debug, Deserialize)]
struct Verification {
    status: VerificationStatus,
}

/// Whether the provider proved the address, in its own spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum VerificationStatus {
    Verified,
    /// `unverified`, `expired`, `failed`, and any status a provider adds later:
    /// only a proof counts, so everything else reads the same.
    #[serde(other)]
    Unproven,
}

impl IdentityEmail {
    fn is_verified(&self) -> bool {
        self.verification
            .as_ref()
            .is_some_and(|proof| proof.status == VerificationStatus::Verified)
    }
}

impl IdentityUser {
    /// The address an account is opened under, or why there is none.
    ///
    /// The one the provider MARKED primary, and only that one. Falling back to
    /// the first address in the list would open an account under whichever
    /// address happened to sort first — a different person's inbox, when a
    /// provider reports several. It must also be verified: accepting an invite
    /// matches on it, so an address nobody proved would hand that address's
    /// invites to whoever typed it.
    pub(super) fn verified_email(&self) -> Result<&str, SignupFailure> {
        let primary = self
            .primary_email_address_id
            .as_deref()
            .and_then(|id| self.email_addresses.iter().find(|address| address.id == id))
            .ok_or(SignupFailure::MissingEmail)?;
        primary
            .is_verified()
            .then_some(primary.email_address.as_str())
            .ok_or(SignupFailure::UnverifiedEmail)
    }

    /// What to call them, when the provider said anything at all.
    pub(super) fn display_name(&self) -> Option<String> {
        let given = self.first_name.as_deref().unwrap_or_default().trim();
        let family = self.last_name.as_deref().unwrap_or_default().trim();
        match (given.is_empty(), family.is_empty()) {
            (true, true) => None,
            (true, false) => Some(family.to_owned()),
            (false, true) => Some(given.to_owned()),
            (false, false) => Some(format!("{given} {family}")),
        }
    }
}
