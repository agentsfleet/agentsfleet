#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use reqwest::Url;

use super::{Target, privacy::Privacy};
use crate::testing::inbound;

#[test]
fn should_preserve_text_and_media_type_for_navigation_redirects() {
    let base = Url::parse("https://storage.example/folder/start").expect("URL");
    for raw in ["/", ".", "./", "..", "../", "?"] {
        let mut privacy = Privacy::default();
        privacy.add(&Target::new(&base, raw).expect("navigation target"));
        let mut response = inbound(200, "failure at src/parser.rs:42. Why?");
        response.content_type = Some("text/plain".to_owned());
        privacy.clean(&mut response).expect("masked");
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
    let base = Url::parse("https://api.github.com/logs").expect("URL");
    let target = Target::new(&base, "https://storage.example/log?sig=a").expect("target");
    let mut privacy = Privacy::default();
    privacy.add(&target);
    for body in [
        "a".repeat(crate::RESPONSE_MAX_BYTES),
        format!("{}a", "z".repeat(crate::RESPONSE_MAX_BYTES - 1)),
        format!("{}€", "λ".repeat(crate::RESPONSE_MAX_BYTES / 2 - 1)),
    ] {
        let mut response = inbound(200, &body);
        privacy
            .clean(&mut response)
            .expect("masked within the response budget");
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
    privacy
        .clean(&mut complete)
        .expect("unchanged complete body");
    assert!(!complete.truncated);
    assert_eq!(complete.body.len(), crate::RESPONSE_MAX_BYTES);
}

#[test]
fn should_refuse_media_type_masking_beyond_the_response_budget() {
    let base = Url::parse("https://api.github.com/logs").expect("URL");
    let mut privacy = Privacy::default();
    privacy.add(&Target::new(&base, "https://storage.example/log?sig=a").expect("target"));
    let mut response = inbound(200, "z");
    response.content_type = Some("a".repeat(crate::RESPONSE_MAX_BYTES));
    assert_eq!(privacy.clean(&mut response), Err(super::Stop::Privacy));
}

#[test]
fn should_keep_signed_redirect_targets_private() {
    let raw = "https://storage.example/private/logs?sig=example%2Bsignature%26value&attempt=1";
    let target = Target::new(
        &Url::parse("https://api.github.com/logs").expect("URL"),
        raw,
    )
    .expect("bounded target");
    assert!(!format!("{target:?}").contains("private"));
    assert!(!format!("{target:?}").contains("signature"));
    let mut privacy = Privacy::default();
    privacy.add(&target);
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
        privacy.clean(&mut response).expect("masked");
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
    let base = Url::parse("https://api.github.com/logs").expect("URL");
    let target = Target::new(
        &base,
        "https://storage.example/a?access_token=%61%62%63&job=9",
    )
    .expect("target");
    let mut privacy = Privacy::default();
    privacy.add(&target);
    let mut response = inbound(200, "%61%62%63 abc job=9 unrelated");
    privacy.clean(&mut response).expect("masked");
    assert!(!response.body.contains("%61%62%63"));
    assert!(!response.body.contains("abc"));
    assert!(response.body.contains("job=9 unrelated"));
}

#[test]
fn should_mask_form_and_uri_component_signature_spellings() {
    let base = Url::parse("https://api.github.com/logs").expect("URL");
    let target = Target::new(&base, "https://storage.example/log?sig=a*b~c%20d").expect("target");
    let mut privacy = Privacy::default();
    privacy.add(&target);
    for value in ["a*b~c d", "a*b%7Ec+d", "a%2Ab~c%20d", "a%2ab~c%20d"] {
        let mut response = inbound(403, &format!("denied: {value}; job=9"));
        privacy.clean(&mut response).expect("masked");
        assert_eq!(response.body, "denied: «secret:redirect»; job=9");
    }
}
