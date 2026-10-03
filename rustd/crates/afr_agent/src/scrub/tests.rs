use std::borrow::Cow;

use super::{Carry, Scrub};
use crate::fixture::{API_KEY, GITHUB_TOKEN, lease, unbounded};

fn scrub() -> Scrub {
    Scrub::new(&lease(&[], unbounded()).policy)
}

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
    let scrub = Scrub::of([("short".to_owned(), "abc"), ("long".to_owned(), "abcdef")]);

    assert_eq!(scrub.text("xabcdefx abc"), "x«secret:long»x «secret:short»");
}

#[test]
fn should_mask_nothing_for_an_empty_value() {
    let scrub = Scrub::of([("empty".to_owned(), "")]);

    assert!(matches!(scrub.text("anything"), Cow::Borrowed("anything")));
    assert_eq!(scrub.pending("anything"), 0);
}

#[test]
fn should_mask_strings_and_keys_inside_json() {
    let mut value = serde_json::json!({
        "headers": {GITHUB_TOKEN: [API_KEY, 7, null]}, "count": 3
    });

    scrub().json(&mut value);

    assert_eq!(
        value,
        serde_json::json!({
            "headers": {"«secret:github.token»": ["«secret:llm.api_key»", 7, null]}, "count": 3
        })
    );
}

#[test]
fn should_hold_back_a_tail_that_could_start_a_secret() {
    let scrub = scrub();

    assert_eq!(scrub.pending("output ghp_fix"), "ghp_fix".len());
    assert_eq!(scrub.pending("output done"), 0);
}

#[test]
fn should_hold_back_whole_characters_only() {
    let scrub = Scrub::of([("emoji".to_owned(), "é-secret")]);
    let text = "café";

    let held = scrub.pending(text);

    assert!(text.is_char_boundary(text.len() - held));
    assert_eq!(held, "é".len());
}

#[test]
fn should_never_send_a_secret_split_across_chunks() {
    let scrub = scrub();
    let (head, tail) = GITHUB_TOKEN.split_at(6);
    let mut carry = Carry::default();

    let first = carry.push(&scrub, &format!("token is {head}"));
    let second = carry.push(&scrub, &format!("{tail} ok"));

    assert_eq!(first, "token is ");
    assert_eq!(second, "«secret:github.token» ok");
    assert!(!format!("{first}{second}").contains(head));
}

#[test]
fn should_drop_a_partial_secret_left_at_the_end() {
    let scrub = scrub();
    let mut carry = Carry::default();

    let sent = carry.push(&scrub, &format!("tail {}", &API_KEY[..5]));

    assert_eq!(sent, "tail ");
    assert!(!carry.held.is_empty(), "held, never sent");
}
