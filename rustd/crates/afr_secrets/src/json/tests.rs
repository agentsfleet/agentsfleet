use serde_json::json;

use super::rewrite;

/// Upper-cases text holding an `x`, and keeps the rest.
fn shout(text: &str) -> Option<String> {
    text.contains('x').then(|| text.to_uppercase())
}

#[test]
fn should_rewrite_every_key_and_string_at_any_depth() {
    let mut value = json!({"xa": ["bx", {"cx": "dx"}, "keep"], "n": 7, "none": null});

    rewrite(&mut value, &shout);

    assert_eq!(
        value,
        json!({"XA": ["BX", {"CX": "DX"}, "keep"], "n": 7, "none": null})
    );
}

#[test]
fn should_leave_a_value_with_no_text_untouched() {
    let mut value = json!([1, true, null, 2.5]);

    rewrite(&mut value, &|_text: &str| Some(String::new()));

    assert_eq!(value, json!([1, true, null, 2.5]));
}
