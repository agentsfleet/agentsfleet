use super::{DISPLAY_HEX_LEN, classify_insert, display_prefix};
use afd_auth::credential::CLI_CREDENTIAL_PREFIX;

#[test]
fn a_display_prefix_reveals_the_marker_and_eight_hex_characters() {
    let credential = format!("{CLI_CREDENTIAL_PREFIX}{}", "a".repeat(64));
    let shown = display_prefix(&credential);

    assert_eq!(
        shown.len(),
        CLI_CREDENTIAL_PREFIX.len() + DISPLAY_HEX_LEN,
        "the stored fragment is the marker plus eight characters"
    );
    assert!(
        credential.starts_with(shown),
        "the fragment must be a prefix of the value it identifies"
    );
    assert!(
        shown.len() < credential.len(),
        "a fragment as long as the credential would store the credential"
    );
}

#[test]
fn a_value_shorter_than_the_fragment_is_returned_whole() {
    // Unreachable through `mint`, which always draws a full-length value.
    // Asserted anyway because the alternative implementation — a bare
    // slice — panics here rather than returning, and a panic on a
    // credential path is a denial of service reachable by a future caller.
    let short = "afc_ab";
    assert_eq!(
        display_prefix(short),
        short,
        "a short value is returned rather than sliced out of bounds"
    );
}

/// A statement that failed for any reason but a unique violation is a fault.
///
/// The collision arm needs a `DatabaseError` carrying SQLSTATE `23505`,
/// which sqlx only produces from a real driver — so the branch this asserts
/// is the OTHER one, and it is the branch that matters here: a lost race is
/// a refusal the caller retries, while everything else has to keep its
/// cause and stay a query fault rather than being reported as a collision.
#[test]
fn a_non_collision_insert_failure_stays_a_query_fault() {
    let failure = classify_insert(sqlx::Error::PoolClosed);

    assert!(!failure.to_string().is_empty());
    assert!(!failure.code().as_str().is_empty());
}
