#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use reqwest::Url;

use super::{Chain, Stop};
use crate::testing::{inbound, redirected};
use crate::{Inbound, Result};

fn read_response(raw: &str, response: &mut Inbound) -> Result<(), Stop> {
    let base = Url::parse("https://api.github.com/folder/start").expect("URL");
    let mut chain = Chain::new(&base, Vec::new());
    assert!(chain.next(&mut redirected(302, &base, raw))?.is_some());
    assert!(chain.next(response)?.is_none());
    chain.finish();
    Ok(())
}

#[test]
fn should_preserve_text_and_media_type_for_navigation_redirects() {
    for raw in ["/", ".", "./", "..", "../", "?"] {
        let mut response = inbound(200, "failure at src/parser.rs:42. Why?");
        response.content_type = Some("text/plain".to_owned());
        read_response(raw, &mut response).expect("masked");
        assert_eq!(response.body, "failure at src/parser.rs:42. Why?", "{raw}");
        assert_eq!(
            response.content_type.as_deref(),
            Some("text/plain"),
            "{raw}"
        );
    }
}

#[test]
fn should_bound_masked_response_without_partial_markers() {
    let target = "https://storage.example/log?sig=a";
    for body in [
        "a".repeat(crate::RESPONSE_MAX_BYTES),
        format!("{}a", "z".repeat(crate::RESPONSE_MAX_BYTES - 1)),
        format!("{}€", "λ".repeat(crate::RESPONSE_MAX_BYTES / 2 - 1)),
    ] {
        let mut response = inbound(200, &body);
        read_response(target, &mut response).expect("masked within the response budget");
        assert!(response.truncated);
        assert!(response.body.len() <= crate::RESPONSE_MAX_BYTES);
        assert!(!response.body.contains('a'));
        assert!(response.body.rfind('«').is_none_or(|start| {
            response
                .body
                .get(start..)
                .is_some_and(|tail| tail.starts_with("«secret:redirect»"))
        }));
    }
    let mut complete = inbound(200, &"z".repeat(crate::RESPONSE_MAX_BYTES));
    read_response(target, &mut complete).expect("unchanged complete body");
    assert!(!complete.truncated);
    assert_eq!(complete.body.len(), crate::RESPONSE_MAX_BYTES);
}

#[test]
fn should_refuse_media_type_masking_beyond_the_response_budget() {
    let mut response = inbound(200, "z");
    response.content_type = Some("a".repeat(crate::RESPONSE_MAX_BYTES));
    assert_eq!(
        read_response("https://storage.example/log?sig=a", &mut response),
        Err(Stop::Privacy)
    );
}

#[test]
fn should_keep_signed_redirect_targets_private() {
    let raw = "https://storage.example/private/logs?sig=example%2Bsignature%26value&attempt=1";
    let target = redirected(
        302,
        &Url::parse("https://api.github.com/logs").expect("URL"),
        raw,
    )
    .redirect
    .expect("bounded target");
    assert!(!format!("{target:?}").contains("private"));
    assert!(!format!("{target:?}").contains("signature"));
    let encoded: String = url::form_urlencoded::byte_serialize(raw.as_bytes()).collect();
    let variants = [
        raw.to_owned(),
        encoded.to_lowercase(),
        raw.replace('/', "\\/"),
        "example+signature&value".to_owned(),
        "example%2bsignature%26value".to_owned(),
    ];
    for (index, value) in variants.into_iter().enumerate() {
        let mut response = inbound(200, &format!("echo={value}; safe attempt=1 job=202"));
        response.content_type = Some(format!("text/plain; debug={value}"));
        assert!(!format!("{response:?}").contains(&value));
        read_response(raw, &mut response).expect("masked");
        assert!(
            !response.body.contains(&value),
            "protected spelling {index} survived"
        );
        assert!(
            !response
                .content_type
                .expect("content type")
                .contains(&value)
        );
        assert!(response.body.contains("safe attempt=1 job=202"));
    }
}

#[test]
fn should_mask_raw_query_values_without_recursively_decoding_other_text() {
    let target = "https://storage.example/a?access_token=%61%62%63&job=9";
    let mut response = inbound(200, "%61%62%63 abc job=9 unrelated");
    read_response(target, &mut response).expect("masked");
    assert!(!response.body.contains("%61%62%63"));
    assert!(!response.body.contains("abc"));
    assert!(response.body.contains("job=9 unrelated"));
}

#[test]
fn should_mask_form_and_uri_component_signature_spellings() {
    let target = "https://storage.example/log?sig=a*b~c%20d";
    for value in ["a*b~c d", "a*b%7Ec+d", "a%2Ab~c%20d", "a%2ab~c%20d"] {
        let mut response = inbound(403, &format!("denied: {value}; job=9"));
        read_response(target, &mut response).expect("masked");
        assert_eq!(response.body, "denied: «secret:redirect»; job=9");
    }
}
