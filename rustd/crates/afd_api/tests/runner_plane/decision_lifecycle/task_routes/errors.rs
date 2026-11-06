//! Transport rejects malformed mutations and queries before native work.
use super::*;
pub(super) async fn refuse_malformed_requests(fixture: &Fixture) {
    let checkpoint = format!("checkpoints/{}", mint_uuid7());
    let page = format!("{checkpoint}/pages/{}", mint_uuid7());
    let pin = format!("{checkpoint}/pins/{}", mint_uuid7());
    // Each mutating public route must reject untyped JSON before native work.
    for (method, resource) in [
        (Method::POST, "bindings".to_owned()),
        (Method::POST, "input-receipts".to_owned()),
        (Method::POST, "effects".to_owned()),
        (Method::POST, "checkpoints".to_owned()),
        (Method::PATCH, checkpoint.clone()),
        (Method::PUT, page.clone()),
        (Method::PATCH, pin),
        (Method::POST, format!("{checkpoint}/objects")),
        (Method::POST, format!("{checkpoint}/uploads")),
        (Method::POST, format!("{checkpoint}/authorizations")),
    ] {
        let body = call(
            fixture,
            method,
            &path(fixture, &resource),
            &json!({"unrecognized":true}),
            StatusCode::BAD_REQUEST,
        )
        .await;
        assert!(body.to_string().contains("UZ-REQ-001"));
    }
    refuse_malformed_reads(fixture, &checkpoint, &page).await;
}

async fn refuse_malformed_reads(fixture: &Fixture, checkpoint: &str, page: &str) {
    for resource in [
        "inputs".to_owned(),
        "effects".to_owned(),
        format!("effects/{}", mint_uuid7()),
        page.to_owned(),
        format!("{checkpoint}/objects"),
    ] {
        let response = send(
            &fixture.router,
            Method::GET,
            &format!("{}?fencing_token=not-a-number", path(fixture, &resource)),
            Some(&fixture.runner.token),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    for limit in [0, 5] {
        let response = send(
            &fixture.router,
            Method::GET,
            &format!(
                "{}?fencing_token={}&limit={limit}",
                path(fixture, "inputs"),
                fixture.lease.fence
            ),
            Some(&fixture.runner.token),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
