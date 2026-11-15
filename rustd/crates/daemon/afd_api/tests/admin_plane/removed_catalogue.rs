//! Retired catalogue URLs stay absent, including for a valid operator.

use super::{PLATFORM_KEY, Scope, StatusCode, fleet};
use crate::harness::send;
use http::Method;

#[tokio::test]
async fn platform_catalogue_routes_are_removed() {
    let operator = fleet(Scope::ModelAdmin);
    for collection in ["/v1/admin/fleet-libraries", "/v1/admin/plugins"] {
        for (suffix, method) in [
            ("", Method::GET),
            ("", Method::POST),
            ("/example", Method::PATCH),
            ("/example", Method::DELETE),
        ] {
            let path = format!("{collection}{suffix}");
            let response = send(&operator, method, &path, Some(PLATFORM_KEY), "{}").await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }
}
