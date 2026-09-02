//! Who a signup names, what it resolved to, and the tenant name it opens under.
//!
//! Split from `signup` because these are the shapes at its edges — what a
//! caller hands in and what it gets back — and they are read by callers that
//! never run a bootstrap.

/// Who is being provisioned.
///
/// A struct rather than three positional `&str`s, which would be mutually
/// assignable: an address in the subject's place would compile and open an
/// account nobody can authenticate as.
#[derive(Debug, Clone, Copy)]
pub struct NewAccount<'a> {
    /// The identity provider's own subject, and the account's unique key.
    pub oidc_subject: &'a str,
    /// The primary address the provider reported.
    pub email: &'a str,
    /// What to call them, when the provider said.
    pub display_name: Option<&'a str>,
}

/// The account a signup resolved to, however it got there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bootstrapped {
    /// The person.
    pub user_id: String,
    /// Their tenant.
    pub tenant_id: String,
    /// Their default workspace.
    pub workspace_id: String,
    /// What that workspace is called.
    pub workspace_name: String,
    /// `true` on a fresh bootstrap, `false` on an idempotent replay.
    pub created: bool,
}

/// The tenant name a personal account is opened under.
///
/// The local part of the address, which is what a person recognises in a
/// workspace switcher. `None` for an address carrying no local part: that is a
/// malformed event rather than a person without a name, and a caller refuses it
/// exactly as it refuses an event carrying no address at all.
///
/// The Zig substitutes a fixed word here instead. That hides an invalid input
/// behind a tenant indistinguishable from any other, and validating at the
/// boundary is this port's rule rather than the Zig's (RULE PORT).
#[must_use]
pub fn personal_tenant_name(email: &str) -> Option<&str> {
    let local = email.split('@').next().unwrap_or_default();
    (!local.is_empty()).then_some(local)
}
