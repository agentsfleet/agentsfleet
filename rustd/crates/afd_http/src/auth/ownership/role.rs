//! Which routes a member of an account is refused inside it.
//!
//! The capability gate has already passed the caller on their own scopes. A
//! member of somebody else's account may hold `secret:write` for their own
//! account, and the role withholds it here: the two owner-grade capabilities
//! mounted on workspace routes are writing secrets and connecting an
//! integration. Everything else a workspace route needs, a member may do.

use afd_auth::scope::Scope;
use afd_tenant::workspace::access::{Grant, Role};

/// The capabilities only an account's owner exercises on workspace routes.
///
/// Every other workspace-route capability is shared with members. Listed by
/// capability rather than by route, so a new route that writes secrets or
/// connects an integration is withheld from members the day it is mounted.
const OWNER_ONLY: &[Scope] = &[Scope::SecretWrite, Scope::ConnectorWrite];

/// Whether `grant` is refused a route whose method requires `required`.
///
/// Only a member is ever refused. An owner holds the account, and a platform
/// crossing is governed by its own scope, which the override already checked.
pub(super) fn withholds(grant: Grant, required: &[Scope]) -> bool {
    matches!(grant, Grant::Membership(Role::Member))
        && required.iter().any(|scope| OWNER_ONLY.contains(scope))
}

#[cfg(test)]
mod tests {
    use afd_auth::scope::Scope;
    use afd_tenant::workspace::access::{Grant, Role};

    use super::withholds;

    const MEMBER: Grant = Grant::Membership(Role::Member);
    const OWNER: Grant = Grant::Membership(Role::Owner);

    #[test]
    fn a_member_is_withheld_only_the_owner_grade_capabilities() {
        for owner_grade in [Scope::SecretWrite, Scope::ConnectorWrite] {
            assert!(withholds(MEMBER, &[owner_grade]), "{}", owner_grade.wire());
        }
        for shared in [
            Scope::FleetRead,
            Scope::FleetWrite,
            Scope::FleetAdmin,
            Scope::SecretRead,
            Scope::ConnectorRead,
            Scope::ScheduleWrite,
            Scope::ApprovalResolve,
            Scope::LibraryWrite,
        ] {
            assert!(!withholds(MEMBER, &[shared]), "{}", shared.wire());
        }
    }

    #[test]
    fn an_owner_and_a_platform_crossing_are_never_withheld() {
        for grant in [OWNER, Grant::Platform] {
            assert!(!withholds(grant, &[Scope::SecretWrite]));
            assert!(!withholds(grant, &[Scope::ConnectorWrite]));
        }
    }

    #[test]
    fn a_route_naming_no_capability_withholds_nothing() {
        assert!(!withholds(MEMBER, &[]));
    }
}
