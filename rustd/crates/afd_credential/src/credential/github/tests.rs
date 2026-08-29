//! The overreach checks, against responses GitHub actually shapes.
//!
//! Every case here is a RESPONSE, because the response is what the check reads.
//! Nothing in this file opens a socket: the exchange is `octocrab`'s and is not
//! what these prove — what they prove is that a token which came back wider
//! than the declaration is thrown away rather than delivered.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::needless_pass_by_value,
    reason = "a test asserts by panicking, and a fixture reads its own JSON by \
              index; the manifest's restriction set is for the daemon"
)]

use afd_fleet_runtime::config::{Access, RepositoryBinding};
use serde_json::json;

use super::{Exchange, Granted, Overreach, Permission, ScopedRequest, installation_id, mint};
use crate::credential::outcome::{Outcome, Retry};
use crate::credential::platform::GithubApp;

/// The repository every case here declares, OWNER-QUALIFIED.
///
/// One spelling, because the owner half is the whole point: the request GitHub
/// receives carries the bare name, so a response naming `<other-owner>/widgets`
/// is the mis-scope these cases exist to catch. Two literals drifting apart
/// would make an overreach case assert against a repository the binding never
/// declared, and it would still pass.
const DECLARED: &str = "acme/widgets";

/// A binding over [`DECLARED`] at `access`.
///
/// Built through the `test-util` seam rather than through a stored config
/// document: what these cases are about is what came BACK from GitHub, and a
/// JSON fixture per case would make every one of them carry a parse that is not
/// the subject.
fn binding(access: Access) -> RepositoryBinding {
    let base = (access == Access::Write).then(|| "main".into());
    RepositoryBinding::from_parts(vec![DECLARED.into()], access, base)
}

/// A response body, as GitHub writes one.
fn granted(permissions: serde_json::Value, repositories: serde_json::Value) -> Granted {
    serde_json::from_value(json!({
        "token": "ghs_fixture",
        "expires_at": "2026-06-26T16:30:00Z",
        "permissions": permissions,
        "repositories": repositories,
    }))
    .expect("the fixture response is well formed")
}

/// One repository, qualified — which is the half the request could not send.
fn repositories() -> serde_json::Value {
    json!([{"full_name": DECLARED}])
}

#[test]
fn test_a_read_binding_asks_for_contents_read_and_nothing_else() {
    let request = ScopedRequest::for_binding(&binding(Access::Read));

    // The owner is stripped, because GitHub scopes by bare name.
    let body = serde_json::to_value(&request).expect("the request serialises");
    assert_eq!(body["repositories"], json!(["widgets"]));
    assert_eq!(body["permissions"], json!({"contents": "read"}));
    // No `pull_requests` entry: its ABSENCE is the read scope.
    assert!(body["permissions"].get("pull_requests").is_none());
}

#[test]
fn test_a_write_binding_additionally_asks_for_pull_requests_write() {
    let request = ScopedRequest::for_binding(&binding(Access::Write));

    let body = serde_json::to_value(&request).expect("the request serialises");
    assert_eq!(
        body["permissions"],
        json!({"contents": "write", "pull_requests": "write"})
    );
}

#[test]
fn test_a_token_reaching_exactly_the_declaration_is_accepted() {
    let binding = binding(Access::Read);
    let request = ScopedRequest::for_binding(&binding);
    // `metadata` rides on every installation token GitHub mints. A read-level
    // extra is ambient and must pass, or no mint would ever succeed.
    let granted = granted(
        json!({"contents": "read", "metadata": "read"}),
        repositories(),
    );

    assert_eq!(granted.verify(&binding, request.permissions()), Ok(()));
}

#[test]
fn test_the_bare_name_mis_scope_is_refused() {
    let binding = binding(Access::Read);
    let request = ScopedRequest::for_binding(&binding);
    // The request could only say `widgets`. GitHub resolved it inside a
    // DIFFERENT account that also has a `widgets`, and said so in `full_name`.
    // This is the whole reason the check reads the response.
    let granted = granted(
        json!({"contents": "read"}),
        json!([{"full_name": "other-org/widgets"}]),
    );

    assert_eq!(
        granted.verify(&binding, request.permissions()),
        Err(Overreach::Repositories)
    );
}

#[test]
fn test_a_token_reaching_more_repositories_than_declared_is_refused() {
    let binding = binding(Access::Read);
    let request = ScopedRequest::for_binding(&binding);
    let granted = granted(
        json!({"contents": "read"}),
        json!([{"full_name": DECLARED}, {"full_name": "acme/secrets"}]),
    );

    assert_eq!(
        granted.verify(&binding, request.permissions()),
        Err(Overreach::Repositories)
    );
}

#[test]
fn test_an_unmodelled_permission_above_read_is_refused() {
    // The case `octocrab::models::InstallationPermissions` cannot express: it
    // has no `administration` field, so deserialising through it would DROP
    // this and the token would be delivered carrying admin write. The open map
    // is what makes it visible.
    let binding = binding(Access::Read);
    let request = ScopedRequest::for_binding(&binding);
    let granted = granted(
        json!({"contents": "read", "administration": "write"}),
        repositories(),
    );

    assert_eq!(
        granted.verify(&binding, request.permissions()),
        Err(Overreach::Permissions)
    );
}

#[test]
fn test_a_permission_level_this_daemon_does_not_model_is_refused() {
    // `Permission::Unknown` sorts above `Write`, so a level GitHub introduces
    // after this was written is refused rather than admitted.
    let binding = binding(Access::Read);
    let request = ScopedRequest::for_binding(&binding);
    let granted = granted(
        json!({"contents": "read", "packages": "maintain"}),
        repositories(),
    );

    assert_eq!(
        granted.verify(&binding, request.permissions()),
        Err(Overreach::Permissions)
    );
}

#[test]
fn test_a_write_token_granted_only_read_is_refused() {
    // A token NARROWER than the declaration fails here, where the fleet can be
    // told why, rather than at the vendor where it cannot.
    let binding = binding(Access::Write);
    let request = ScopedRequest::for_binding(&binding);
    let granted = granted(
        json!({"contents": "read", "pull_requests": "read"}),
        repositories(),
    );

    assert_eq!(
        granted.verify(&binding, request.permissions()),
        Err(Overreach::Permissions)
    );
}

#[test]
fn test_a_response_stating_no_reach_is_refused_not_assumed() {
    let binding = binding(Access::Read);
    let request = ScopedRequest::for_binding(&binding);

    // No `repositories` array at all.
    let no_repositories: Granted = serde_json::from_value(json!({
        "token": "ghs_fixture",
        "permissions": {"contents": "read"},
    }))
    .expect("a response may omit repositories");
    assert_eq!(
        no_repositories.verify(&binding, request.permissions()),
        Err(Overreach::Unstated)
    );

    // And no permissions object.
    let no_permissions = granted(json!({}), repositories());
    assert_eq!(
        no_permissions.verify(&binding, request.permissions()),
        Err(Overreach::Unstated)
    );
}

#[test]
fn test_permission_levels_order_so_that_stronger_is_greater() {
    // The ordering IS the check, so it is asserted rather than assumed.
    assert!(Permission::Read < Permission::Write);
    assert!(Permission::Write < Permission::Admin);
    assert!(Permission::Admin < Permission::Unknown);
}

#[test]
fn installation_ids_accept_only_unsigned_numbers_and_decimal_strings() {
    assert_eq!(installation_id(&json!(42)), Some(42));
    assert_eq!(installation_id(&json!("42")), Some(42));
    for invalid in [json!(-1), json!("not-a-number"), json!({}), json!([])] {
        assert_eq!(installation_id(&invalid), None);
    }
}

#[tokio::test]
async fn mint_refuses_before_transport_when_narrowing_inputs_are_unusable() {
    let app = GithubApp {
        app_id: 7,
        private_key_pem: "not a private key".to_owned().into(),
    };
    let usable_handle = json!({"installation_id": 42});
    let repository = binding(Access::Read);

    let absent_installation = mint(Exchange {
        app: &app,
        handle: &json!({}),
        binding: Some(&repository),
        now_ms: 1,
    })
    .await;
    assert!(matches!(absent_installation, Outcome::ReconnectRequired));

    let absent_binding = mint(Exchange {
        app: &app,
        handle: &usable_handle,
        binding: None,
        now_ms: 1,
    })
    .await;
    assert!(matches!(
        absent_binding,
        Outcome::MintFailed(Retry::Permanent)
    ));

    let bad_key = mint(Exchange {
        app: &app,
        handle: &usable_handle,
        binding: Some(&repository),
        now_ms: 1,
    })
    .await;
    assert!(matches!(bad_key, Outcome::MintFailed(Retry::Permanent)));
}

mod transport;
