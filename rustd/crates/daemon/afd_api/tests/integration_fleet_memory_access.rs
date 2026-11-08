//! Dimension 4.2: the memory-access route sets both grants under `fleet:write`,
//! answers the same reply to the same body, and refuses a `fleet:read` token.
#![cfg(feature = "test-util")]

use crate::harness;
use crate::integration_fleet_memories::{Fixture, SUBJECT};

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::{Value, json};

use self::harness::{Fleet, json_body, send};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_memory_access_route_needs_fleet_write() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let path = format!(
        "/v1/workspaces/{}/fleets/{}/memory-access",
        fixture.workspace.as_str(),
        fixture.fleet
    );
    let token = Some(fixture.token.as_str());
    let router = |scopes: &[Scope]| {
        Fleet::live(
            fixture.database.clone(),
            SUBJECT,
            ScopeSet::from_scopes(scopes),
        )
        .with_owned_workspace(fixture.workspace.clone())
        .router()
    };
    let admin = router(&Scope::ALL);
    let both = json!({"read": true, "publish": true}).to_string();

    let granted = send(&admin, Method::PATCH, &path, token, &both).await;
    assert_eq!(granted.status(), StatusCode::OK);
    let granted: Value = json_body(granted).await;
    assert_eq!(
        granted,
        json!({"read": true, "publish": true}),
        "both grants set"
    );
    let again = json_body(send(&admin, Method::PATCH, &path, token, &both).await).await;
    assert_eq!(again, granted, "the same body twice answers the same reply");

    let one = json!({"publish": false}).to_string();
    let narrowed = json_body(send(&admin, Method::PATCH, &path, token, &one).await).await;
    assert_eq!(
        narrowed,
        json!({"read": true, "publish": false}),
        "an absent grant is kept"
    );

    let reader = router(&[Scope::FleetRead]);
    let refused = send(&reader, Method::PATCH, &path, token, &both).await;
    assert_eq!(
        refused.status(),
        StatusCode::FORBIDDEN,
        "fleet:read may not grant"
    );
    fixture.cleanup().await;
}
