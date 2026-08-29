//! Secret name and body validation before datastore access.

use super::*;

#[tokio::test]
async fn a_body_that_is_not_a_non_empty_object_never_reaches_the_store() {
    // One constructor decides this for both writing verbs, so both are asserted
    // against the same table — that is the property the split-file version of
    // this suite would have hidden.
    for data in ["{}", r#""a string""#, "[]", r#"["a","b"]"#, "42", "null"] {
        let create = authorised(
            Method::POST,
            &collection(),
            &format!(r#"{{"name":"{SECRET}","data":{data}}}"#),
        )
        .await;
        assert_eq!(create.status(), StatusCode::BAD_REQUEST, "create {data}");
        assert_eq!(code_of(create).await, "UZ-VAULT-001", "create {data}");

        let replace = authorised(Method::PUT, &item(), &format!(r#"{{"data":{data}}}"#)).await;
        assert_eq!(replace.status(), StatusCode::BAD_REQUEST, "replace {data}");
        assert_eq!(code_of(replace).await, "UZ-VAULT-001", "replace {data}");
    }
}

#[tokio::test]
async fn a_body_past_four_kibibytes_is_refused_with_its_own_code() {
    // A distinct code from the shape refusal, because the remedies differ: one
    // caller has the wrong kind of value, the other has too much of it.
    let oversized = format!(
        r#"{{"name":"{SECRET}","data":{{"k":"{}"}}}}"#,
        "v".repeat(MAX_DATA_BYTES)
    );
    let response = authorised(Method::POST, &collection(), &oversized).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(response).await, "UZ-VAULT-002");
}

#[tokio::test]
async fn whitespace_the_caller_sent_does_not_count_against_the_bound() {
    // The bound is measured on the canonical form, which is what gets stored.
    // Counting the request bytes would refuse a body that fits.
    let padded = format!(
        r#"{{"name":"{SECRET}","data":{{ "k"{} : "v" }}}}"#,
        " ".repeat(MAX_DATA_BYTES * 2)
    );

    assert_reached_the_verb(
        authorised(Method::POST, &collection(), &padded).await,
        "a padded body within the canonical bound",
    )
    .await;
}

#[tokio::test]
async fn a_name_outside_its_bounds_never_reaches_the_store() {
    // The create takes its name from the BODY and the replace from the PATH,
    // and both answer through `SecretName::parse` — so the two cannot come to
    // disagree about what a storable name is.
    let too_long = "n".repeat(65);

    for name in ["", too_long.as_str()] {
        let create = authorised(
            Method::POST,
            &collection(),
            &format!(r#"{{"name":"{name}","data":{{"k":"v"}}}}"#),
        )
        .await;
        assert_eq!(create.status(), StatusCode::BAD_REQUEST, "create {name:?}");
        assert_eq!(code_of(create).await, "UZ-REQ-001", "create {name:?}");
    }

    // The empty half has no path form — `/secrets/` is a different template —
    // so only the over-long name is reachable through the item route.
    let path = format!("/v1/workspaces/{OWNED_WORKSPACE}/secrets/{too_long}");
    for (method, body) in [(Method::PUT, VALID_REPLACE), (Method::DELETE, "")] {
        let response = authorised(method.clone(), &path, body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{method}");
        assert_eq!(code_of(response).await, "UZ-REQ-001", "{method}");
    }
}

#[tokio::test]
async fn a_body_this_daemon_cannot_read_is_told_apart_from_one_that_is_absent() {
    // Two different sentences, because the remedies differ. The fleet install
    // defaults an empty body to `{}` — every field there is optional — and here
    // there would be no secret to store.
    let malformed = authorised(Method::POST, &collection(), "{not json").await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        detail_of(malformed).await,
        afd_api::handler::secret::DETAIL_MALFORMED_JSON
    );

    let absent = authorised(Method::POST, &collection(), "").await;
    assert_eq!(absent.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        detail_of(absent).await,
        afd_api::handler::secret::DETAIL_BODY_REQUIRED
    );
}

#[tokio::test]
async fn a_create_body_missing_its_name_or_its_data_is_refused() {
    // Both fields are required and neither has a default: a secret with no name
    // has no address, and one with no body has nothing to seal.
    for body in [
        r#"{"data":{"k":"v"}}"#,
        &format!(r#"{{"name":"{SECRET}"}}"#),
        "{}",
    ] {
        let response = authorised(Method::POST, &collection(), body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(
            detail_of(response).await,
            afd_api::handler::secret::DETAIL_MALFORMED_JSON,
            "{body}"
        );
    }
}

#[tokio::test]
async fn the_item_route_answers_no_get() {
    // There is no read handler on this surface and never will be: a stored
    // secret is not readable. A 405 rather than a 404 is what says the path
    // exists and the verb does not.
    let response = authorised(Method::GET, &item(), "").await;

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
