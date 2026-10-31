#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use reqwest::Url;

use super::{Target, privacy::Privacy};
use crate::testing::inbound;

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
