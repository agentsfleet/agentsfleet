use super::*;

#[test]
fn should_mask_the_provider_key_and_every_credential_field() {
    let masked = scrub()
        .text(&format!("{API_KEY} then {GITHUB_TOKEN}"))
        .into_owned();

    assert_eq!(masked, "«secret:llm.api_key» then «secret:github.token»");
}

#[test]
fn should_leave_a_credentials_host_unmasked() {
    let text = "GET https://api.github.com/repos";

    assert!(matches!(scrub().text(text), Cow::Borrowed(_)));
}

#[test]
fn should_mask_the_longer_secret_when_one_contains_another() {
    let scrub = Scrub::of([("short".to_owned(), "abc"), ("long".to_owned(), "abcdef")]).unwrap();

    assert_eq!(scrub.text("xabcdefx abc"), "x«secret:long»x «secret:short»");
}

#[test]
fn should_mask_nothing_for_an_empty_value() {
    let scrub = Scrub::of([("empty".to_owned(), "")]).unwrap();

    assert!(matches!(scrub.text("anything"), Cow::Borrowed("anything")));
    assert_eq!(scrub.pending("anything"), 0);
}

#[test]
fn should_mask_strings_and_keys_inside_json() {
    let value = serde_json::json!({
        "headers": {GITHUB_TOKEN: [API_KEY, 7, null]}, "count": 3
    });

    let masked = scrub().clean_json(value).into_inner();

    assert_eq!(
        masked,
        serde_json::json!({
            "headers": {"«secret:github.token»": ["«secret:llm.api_key»", 7, null]}, "count": 3
        })
    );
}

#[test]
fn should_replace_a_nul_the_store_cannot_hold_in_text_keys_and_strings() {
    let scrub = scrub();

    assert_eq!(*scrub.clean("bin\0ary".to_owned()), "bin\u{fffd}ary");
    assert_eq!(
        scrub
            .clean_json(serde_json::json!({"k\0": ["v\0", 1]}))
            .into_inner(),
        serde_json::json!({"k\u{fffd}": ["v\u{fffd}", 1]})
    );
}
