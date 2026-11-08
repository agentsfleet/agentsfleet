use super::*;

#[test]
fn should_hold_back_a_tail_that_could_start_a_secret() {
    let scrub = scrub();

    assert_eq!(scrub.pending("output ghp_fix"), "ghp_fix".len());
    assert_eq!(scrub.pending("output done"), 0);
}

#[test]
fn should_hold_back_whole_characters_only() {
    let scrub = Scrub::of([("emoji".to_owned(), "é-secret")]).unwrap();
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
    assert_eq!(format!("{carry:?}"), "Carry { held_bytes: 5 }");
}

#[test]
fn should_hand_back_the_held_buffer_unmasked_when_nothing_matched() {
    let mut carry = Carry::default();

    let ready = carry.push(&scrub(), "plain text with no secret");

    assert_eq!(ready, "plain text with no secret");
    assert_eq!(format!("{carry:?}"), "Carry { held_bytes: 0 }");
}

#[test]
fn should_print_a_held_tail_as_its_length_only() {
    let scrub = Scrub::of([("github.token".to_owned(), "ghs_live_secret")]).unwrap();
    let mut carry = Carry::default();

    let sent = carry.push(&scrub, "ghs_li");

    assert_eq!(sent, "");
    let printed = format!("{carry:?} {scrub:?}");
    assert!(!printed.contains("ghs_li"), "{printed}");
    assert!(!printed.contains("ghs_live_secret"), "{printed}");
}

#[test]
fn should_hold_a_completed_secret_that_may_still_start_a_longer_one() {
    let scrub = Scrub::of([("short".to_owned(), "abc"), ("long".to_owned(), "abcdef")]).unwrap();
    let mut carry = Carry::default();

    let first = carry.push(&scrub, "token abc");
    let second = carry.push(&scrub, "def done");
    let sent = format!("{first}{second}");

    assert_eq!(sent, "token «secret:long» done");
    assert!(!sent.contains("def"), "{sent}");
}

#[test]
fn should_release_a_completed_short_secret_once_the_stream_moves_past_it() {
    let scrub = Scrub::of([("short".to_owned(), "abc"), ("long".to_owned(), "abcdef")]).unwrap();
    let mut carry = Carry::default();

    let first = carry.push(&scrub, "token abc");
    let second = carry.push(&scrub, " then more");

    assert_eq!(format!("{first}{second}"), "token «secret:short» then more");
}

#[test]
fn should_never_release_a_secret_whose_tail_starts_another() {
    // "tok9g" ends with the byte "ghp_x" starts with, so the hold-back cut
    // would land inside the first secret.
    let scrub = Scrub::of([("a".to_owned(), "tok9g"), ("b".to_owned(), "ghp_x")]).unwrap();
    let mut carry = Carry::default();

    let first = carry.push(&scrub, "key tok9g");
    let second = carry.push(&scrub, " done");

    assert_eq!(format!("{first}{second}"), "key «secret:a» done");
}

#[test]
fn should_never_release_a_secret_whose_last_byte_is_its_first() {
    let scrub = Scrub::of([("k".to_owned(), "sk_live_abc_s")]).unwrap();
    let mut carry = Carry::default();

    let first = carry.push(&scrub, "echo sk_live_abc_s");
    let second = carry.push(&scrub, " done");

    assert_eq!(format!("{first}{second}"), "echo «secret:k» done");
}
