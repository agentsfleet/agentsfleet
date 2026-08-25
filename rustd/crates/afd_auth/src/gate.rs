//! The single capability gate.
//!
//! One check, composed after whichever middleware proved the credential. The
//! route's required scopes are an ANY-OF set: the caller is allowed if they
//! hold at least one. An empty requirement means the route names no capability
//! — authenticated-only — and passes once a principal exists.
//!
//! Ownership is a separate axis and is not decided here. Holding `fleet:write`
//! says nothing about whether the caller owns the workspace they are pointing
//! at, and collapsing the two would make a capability into a key to everyone's
//! data.

use afd_core::error_code::ErrorCode;

use crate::principal::Principal;
use crate::scope::{INSUFFICIENT_SCOPE, Scope};

/// A refusal: the caller authenticated and holds none of the required scopes.
///
/// It carries the whole requirement rather than one scope the caller lacks,
/// because the gate is any-of — naming a single missing scope would tell a
/// caller to obtain that one when any of the others would also have let them
/// through. The Zig daemon renders the same set the same way
/// (`"Requires scope a or b"`), and this is the text a client sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Denied {
    required: &'static [Scope],
}

impl Denied {
    /// The scopes, any one of which would have allowed the request.
    #[must_use]
    pub const fn required(self) -> &'static [Scope] {
        self.required
    }

    /// The registry code this refusal answers with — always `UZ-AUTH-022`.
    ///
    /// A 403. The caller proved who they are, so re-authenticating cannot help
    /// and a 401 would send them round a loop that never terminates.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        INSUFFICIENT_SCOPE
    }
}

impl std::fmt::Display for Denied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Requires scope ")?;
        for (index, scope) in self.required.iter().enumerate() {
            if index > 0 {
                f.write_str(" or ")?;
            }
            f.write_str(scope.wire())?;
        }
        Ok(())
    }
}

impl std::error::Error for Denied {}

/// Allows the request iff `principal` holds at least one of `required`.
///
/// # Errors
/// Returns [`Denied`] naming the whole requirement when the principal holds
/// none of it. An empty `required` never refuses.
///
/// # Fail-closed
///
/// A principal whose claim was absent, unparseable, or resolved from a subject
/// the provider no longer knows carries the empty set, so every non-empty
/// requirement refuses. That is the direction this must fail in, and it falls
/// out of `parse_claim` returning an empty set rather than an error — there is
/// no path where a broken claim reaches here as a permissive one.
pub const fn require_scope(
    principal: &Principal,
    required: &'static [Scope],
) -> Result<(), Denied> {
    if principal.scopes().satisfies_any(required) {
        Ok(())
    } else {
        Err(Denied { required })
    }
}
