#![expect(
    clippy::expect_used,
    reason = "test fixtures must fail loudly when malformed"
)]

use super::*;

const VALID: &[u8] = br#"{"provider":"anthropic","model_id":"claude-opus-5","context_cap_tokens":200000,"input_nanos_per_mtok":5,"cached_input_nanos_per_mtok":1,"output_nanos_per_mtok":25}"#;

#[test]
fn create_validation_pins_every_bound() {
    assert_eq!(create_request(VALID).map(|_request| ()), Ok(()));
    assert_eq!(create_request(b""), Err(DETAIL_BODY_REQUIRED));
    assert_eq!(create_request(b"[]"), Err(DETAIL_MALFORMED_JSON));
    assert_eq!(
            create_request(br#"{"provider":"","model_id":"x","context_cap_tokens":1,"input_nanos_per_mtok":0,"cached_input_nanos_per_mtok":0,"output_nanos_per_mtok":0}"#),
            Err(DETAIL_PROVIDER_LEN)
        );
    assert_eq!(
            create_request(br#"{"provider":"x","model_id":"x","context_cap_tokens":0,"input_nanos_per_mtok":0,"cached_input_nanos_per_mtok":0,"output_nanos_per_mtok":0}"#),
            Err(DETAIL_CAP_POSITIVE)
        );
    assert_eq!(
            create_request(br#"{"provider":"x","model_id":"x","context_cap_tokens":1,"input_nanos_per_mtok":-1,"cached_input_nanos_per_mtok":0,"output_nanos_per_mtok":0}"#),
            Err(DETAIL_RATES_NONNEGATIVE)
        );
}

#[test]
fn inference_validation_names_the_field_in_create_and_patch() {
    let mut body: serde_json::Value = serde_json::from_slice(VALID).expect("fixture JSON");
    for (field, invalid, expected) in [
        ("base_url", serde_json::json!(""), DETAIL_INFERENCE_URL),
        ("dialect", serde_json::json!(""), DETAIL_INFERENCE_DIALECT),
        (
            "max_output_tokens",
            serde_json::json!(0),
            DETAIL_INFERENCE_OUTPUT,
        ),
    ] {
        let mut inference = serde_json::json!({"api":"chat","base_url":"https://models.example/v1","max_output_tokens":4096});
        inference
            .as_object_mut()
            .expect("inference object")
            .insert(field.into(), invalid);
        body.as_object_mut()
            .expect("body object")
            .insert("inference".into(), inference);
        let encoded = serde_json::to_vec(&body).expect("body JSON");
        assert_eq!(create_request(&encoded).map(|_| ()), Err(expected));
        let mut patch = body.clone();
        patch.as_object_mut().expect("object").remove("provider");
        patch.as_object_mut().expect("object").remove("model_id");
        assert_eq!(
            rates_request(&serde_json::to_vec(&patch).expect("patch JSON")).map(|_| ()),
            Err(expected)
        );
    }
}

#[test]
fn patch_requires_a_bounded_rates_body() {
    assert_eq!(rates_request(b"").map(|_| ()), Err(DETAIL_BODY_REQUIRED));
    assert_eq!(rates_request(b"[]").map(|_| ()), Err(DETAIL_MALFORMED_JSON));
    let mut body: serde_json::Value = serde_json::from_slice(VALID).expect("fixture JSON");
    body.as_object_mut().expect("object").remove("provider");
    body.as_object_mut().expect("object").remove("model_id");
    assert_eq!(
        rates_request(&serde_json::to_vec(&body).expect("patch JSON")).map(|_| ()),
        Ok(())
    );
    body.as_object_mut()
        .expect("body object")
        .insert(FIELD_CONTEXT_CAP.into(), serde_json::json!(0));
    assert_eq!(
        rates_request(&serde_json::to_vec(&body).expect("patch JSON")).map(|_| ()),
        Err(DETAIL_CAP_POSITIVE)
    );
}
