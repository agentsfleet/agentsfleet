//! What a fleet's approved grants admit, read without a datastore.

use afd_api_wire::grant::grant_type;

use super::{Approved, ApprovedGrant};
use crate::policy::egress::CREDENTIAL_GITHUB;

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn grant(name: &str, grant_type: &str, scope: Option<&str>) -> ApprovedGrant {
    ApprovedGrant {
        name: name.to_owned(),
        grant_type: grant_type.to_owned(),
        scope: scope.map(str::to_owned),
    }
}

const READ_SCOPE: &str = r#"{"repositories":["acme/widgets"],"access":"read"}"#;

#[test]
fn a_fleet_approved_nothing_holds_nothing_and_binds_no_repository() -> Outcome {
    let approved = Approved::none();
    assert_eq!(approved.names(), [] as [&str; 0]);
    assert!(!approved.holds("sentry", grant_type::SECRET));
    assert_eq!(approved.repository_binding()?, None);
    Ok(())
}

#[test]
fn a_grant_is_held_by_its_exact_name_and_kind() {
    let approved: Approved = std::iter::once(grant("sentry", grant_type::SECRET, None)).collect();
    assert_eq!(approved.names(), ["sentry"]);
    assert!(approved.holds("sentry", grant_type::SECRET));
    assert!(!approved.holds("sentry", grant_type::INTEGRATION));
    assert!(!approved.holds("sentry-staging", grant_type::SECRET));
}

#[test]
fn the_approved_github_scope_builds_the_binding() -> Outcome {
    let approved: Approved = [
        grant("sentry", grant_type::SECRET, None),
        grant(CREDENTIAL_GITHUB, grant_type::INTEGRATION, Some(READ_SCOPE)),
    ]
    .into_iter()
    .collect();
    let binding = approved
        .repository_binding()?
        .ok_or("an approved GitHub scope binds a repository")?;
    assert_eq!(
        binding.repositories(),
        ["acme/widgets".into()] as [Box<str>; 1]
    );
    assert_eq!(binding.access(), af_fleet_runtime::config::Access::Read);
    Ok(())
}

#[test]
fn a_scope_on_another_name_binds_nothing() -> Outcome {
    let approved: Approved =
        std::iter::once(grant("gitlab", grant_type::INTEGRATION, Some(READ_SCOPE))).collect();
    assert_eq!(approved.repository_binding()?, None);
    Ok(())
}

#[test]
fn an_unreadable_stored_scope_fails_the_run_until_asked_again() -> Outcome {
    for stored in ["", "{}", r#"{"repositories":["acme/widgets"]}"#, "not json"] {
        let approved: Approved = std::iter::once(grant(
            CREDENTIAL_GITHUB,
            grant_type::INTEGRATION,
            Some(stored),
        ))
        .collect();
        let Err(refused) = approved.repository_binding() else {
            return Err(format!("{stored:?} must not bind a repository").into());
        };
        assert_eq!(
            refused.code(),
            af_core::error_code::INTERNAL_OPERATION_FAILED,
            "{stored:?}"
        );
    }
    Ok(())
}

#[test]
fn host_grants_never_resolve_credentials_or_repository_scope() -> Outcome {
    let granted: Approved = [
        grant("docs.example", grant_type::HOST, None),
        grant(CREDENTIAL_GITHUB, grant_type::HOST, Some(READ_SCOPE)),
    ]
    .into_iter()
    .collect();
    assert_eq!(granted.names(), [] as [&str; 0]);
    assert_eq!(granted.repository_binding()?, None);
    assert_eq!(
        granted.hosts().collect::<Vec<_>>(),
        ["docs.example", CREDENTIAL_GITHUB]
    );
    Ok(())
}
