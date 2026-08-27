//! Platform-route capability matrix over every declared admin verb.
#![expect(clippy::expect_used, reason = "tests inspect gate decisions")]

use afd_api::route::{AdminRoute, Verb};
use afd_auth::principal::{Person, PersonCredential, Principal, Subject};
use afd_auth::require_scope;
use afd_auth::scope::parse_claim;
use afd_core::id::Uuid7;
use http::Method;

const TENANT_ID: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";

fn principal(credential: PersonCredential, claim: &str) -> Principal {
    Principal::Person(Person::new(
        credential,
        Uuid7::parse(TENANT_ID).expect("fixture tenant is canonical"),
        Subject::new("user_platform_test").expect("fixture subject is valid"),
        parse_claim(claim),
    ))
}

fn method(verb: Verb) -> &'static Method {
    match verb {
        Verb::Get => &Method::GET,
        Verb::Post => &Method::POST,
        Verb::Put => &Method::PUT,
        Verb::Patch => &Method::PATCH,
        Verb::Delete => &Method::DELETE,
    }
}

#[test]
fn test_admin_scope_gates() {
    let tenant_session = principal(
        PersonCredential::SessionToken {
            workspace_scope: Some(Uuid7::parse(TENANT_ID).expect("fixture workspace is canonical")),
        },
        "fleet:admin secret:write",
    );
    let tenant_key = principal(PersonCredential::TenantApiKey, "fleet:admin secret:write");

    for route in AdminRoute::ALL {
        for verb in route.verbs() {
            let required = route.meta().scopes.required(method(*verb));
            for foreign in [&tenant_session, &tenant_key] {
                let denied = require_scope(foreign, required)
                    .expect_err("tenant-only authority cannot reach an admin route");
                assert_eq!(denied.code().as_str(), "UZ-AUTH-022");
            }

            let claim = required
                .first()
                .expect("every admin route has a platform scope")
                .wire();
            let platform = principal(
                PersonCredential::SessionToken {
                    workspace_scope: None,
                },
                claim,
            );
            require_scope(&platform, required).expect("the platform scope passes");
        }
    }
}
