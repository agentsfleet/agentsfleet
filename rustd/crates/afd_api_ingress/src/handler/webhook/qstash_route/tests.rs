//! What a fire naming no schedule of this daemon's logs about the id it named.

use super::{MAX_LOGGED_ID_BYTES, logged_id};

#[test]
fn an_id_the_scheduler_chose_is_logged_whole() {
    assert_eq!(
        logged_id(Some("scd_62RMNMs9teSq1WGniPfFHCTKUyg7")),
        "scd_62RMNMs9teSq1WGniPfFHCTKUyg7"
    );
}

#[test]
fn a_missing_header_logs_nothing() {
    assert_eq!(logged_id(None), "");
}

#[test]
fn an_oversized_header_is_cut_at_the_cap() {
    // The header is outside the signature; its sender chooses its length.
    let sent = "a".repeat(MAX_LOGGED_ID_BYTES * 4);

    assert_eq!(logged_id(Some(&sent)), &sent[..MAX_LOGGED_ID_BYTES]);
}

#[test]
fn an_oversized_header_of_wide_characters_is_cut_at_a_character_boundary() {
    // Two bytes each, so the byte cap falls inside a character when it is odd
    // and on a boundary when it is even; either way the cut never exceeds it.
    let sent = "\u{e9}".repeat(MAX_LOGGED_ID_BYTES);

    let logged = logged_id(Some(&sent));

    assert!(
        logged.len() <= MAX_LOGGED_ID_BYTES,
        "the cap holds: {}",
        logged.len()
    );
    assert!(
        !logged.is_empty(),
        "a cut that falls inside a character backs off, not to nothing"
    );
    assert!(
        sent.starts_with(logged),
        "the logged id is a prefix of what was sent"
    );
}
