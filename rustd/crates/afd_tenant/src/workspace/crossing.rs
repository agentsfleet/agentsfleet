//! The audit record a platform crossing leaves before it is honoured.
//!
//! [`super::Workspaces::authorize`] answers [`Grant::Platform`] and records
//! nothing, because the same decision is re-asked by every open stream on its
//! beat: one crossing is one request, not one record per beat. The caller that
//! HONOURS the crossing records it here, first, with the one fact only it can
//! see, the request's method. That is the ownership layer for every workspace
//! route, and the connector callback that re-authorizes outside it.
//!
//! `warn`, and it earns it: an operator crossing a tenant boundary is rare,
//! legitimate, and exactly what somebody reviewing an incident needs to find.
//! The method is what tells that reviewer a look from an act.

use afd_auth::principal::Principal;
use afd_core::id::Uuid7;

use super::access::{Access, Grant};

/// The event every honoured crossing is recorded under.
pub const EVENT_CROSSING: &str = "cross_tenant_workspace_override";

/// Records `access` to `workspace`, reached with `method`, when it crosses a
/// tenant boundary; records nothing for access from inside the account.
///
/// Called BEFORE the request is honoured, so a crash between the decision and
/// the work still leaves the record.
pub fn audit(principal: &Principal, access: &Access, workspace: &Uuid7, method: &str) {
    if access.grant != Grant::Platform {
        return;
    }
    // Hoisted fields: the `log` bridge duplicates every expression and
    // llvm-cov scores the dead copy (`docs/LOGGING_STANDARD.md` §8A).
    let subject = principal.person().map(|person| person.subject().as_str());
    let acting_tenant = principal.tenant().map(Uuid7::as_str);
    let target_tenant = access.tenant.as_str();
    let target_workspace = workspace.as_str();
    tracing::warn!(
        subject,
        acting_tenant,
        target_tenant,
        target_workspace,
        method,
        event = EVENT_CROSSING,
        "a platform-scoped principal reached another tenant's workspace"
    );
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]
mod tests {
    use afd_auth::principal::{Person, PersonCredential, Principal, Subject};
    use afd_auth::scope::{Scope, ScopeSet};
    use afd_core::id::Uuid7;
    use afd_core::test_util::trace::Capture;
    use tracing::Level;

    use super::{EVENT_CROSSING, audit};
    use crate::workspace::access::{Access, Grant, Role};

    const OPERATOR_TENANT: &str = "0190f2d4-0000-7000-8000-000000000001";
    const TARGET_TENANT: &str = "0190f2d4-0000-7000-8000-000000000002";
    const WORKSPACE: &str = "0190f2d4-0000-7000-8000-000000000003";
    const SUBJECT: &str = "user_2platform_operator";

    fn id(text: &str) -> Uuid7 {
        Uuid7::parse(text).expect("the fixture identifier is canonical")
    }

    fn operator() -> Principal {
        Principal::Person(Person::new(
            PersonCredential::CliCredential,
            id(OPERATOR_TENANT),
            Subject::new(SUBJECT).expect("the fixture subject is not blank"),
            ScopeSet::from_scopes(&[Scope::WorkspaceAny]),
        ))
    }

    fn access(grant: Grant) -> Access {
        Access {
            tenant: id(TARGET_TENANT),
            grant,
        }
    }

    #[test]
    fn test_platform_crossing_audited() {
        let capture = Capture::install();
        audit(
            &operator(),
            &access(Grant::Platform),
            &id(WORKSPACE),
            "POST",
        );

        let event = capture.only(EVENT_CROSSING);
        assert_eq!(event.level, Level::WARN);
        for (field, expected) in [
            ("method", "POST"),
            ("subject", SUBJECT),
            ("acting_tenant", OPERATOR_TENANT),
            ("target_tenant", TARGET_TENANT),
            ("target_workspace", WORKSPACE),
        ] {
            assert_eq!(
                event.fields.get(field).map(String::as_str),
                Some(expected),
                "{field}"
            );
        }
    }

    #[test]
    fn access_from_inside_the_account_is_not_a_crossing() {
        let capture = Capture::install();
        for role in [Role::Owner, Role::Member] {
            audit(
                &operator(),
                &access(Grant::Membership(role)),
                &id(WORKSPACE),
                "POST",
            );
        }
        assert!(capture.events().is_empty(), "{:?}", capture.events());
    }
}
