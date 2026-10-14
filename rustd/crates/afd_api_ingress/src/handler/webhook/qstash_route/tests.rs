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
