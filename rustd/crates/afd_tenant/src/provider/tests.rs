#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test precondition that does not hold must fail loudly, not be handled"
)]

//! What a posture can and cannot be built out of.
//!
//! These are about the type's guarantee rather than its getters: every test
//! below names a state the rest of the crate is entitled to assume is
//! impossible, so the assertions are on what construction REFUSES at least as
//! much as on what it accepts.

use super::{
    MODE_PLATFORM, MODE_SELF_MANAGED, MalformedSecretRef, Posture, SecretRef, StoredPosture,
};

/// The name a well-formed self-managed row carries.
const A_KEY: &str = "openai-primary";

#[test]
fn a_credential_name_that_names_nothing_is_refused() {
    assert_eq!(SecretRef::parse(""), Err(MalformedSecretRef::Blank));
    assert_eq!(SecretRef::parse("   "), Err(MalformedSecretRef::Blank));
    assert_eq!(SecretRef::parse("\t\n"), Err(MalformedSecretRef::Blank));
}

#[test]
fn a_credential_name_is_stored_without_its_surrounding_whitespace() {
    let parsed = SecretRef::parse("  openai-primary  ").expect("a padded name is still a name");
    assert_eq!(parsed.as_str(), A_KEY);
}

#[test]
fn a_credential_name_over_the_vault_ceiling_reports_how_far_over() {
    let length = SecretRef::MAX_BYTES + 1;
    assert_eq!(
        SecretRef::parse(&"k".repeat(length)),
        Err(MalformedSecretRef::TooLong { length })
    );
    let at_the_ceiling = "k".repeat(SecretRef::MAX_BYTES);
    assert_eq!(
        SecretRef::parse(&at_the_ceiling)
            .as_ref()
            .map(SecretRef::as_str),
        Ok(at_the_ceiling.as_str()),
        "the ceiling itself is accepted — the refusal starts one byte later"
    );
}

#[test]
fn platform_mode_carries_no_credential_even_when_the_row_holds_one() {
    // A row written before the column was cleared, or by hand. Platform mode
    // means the shared key whatever else the row says, so the stray name is
    // dropped rather than making the read fail.
    let posture = Posture::from_columns(MODE_PLATFORM, Some(A_KEY)).expect("platform always reads");
    assert_eq!(posture, Posture::Platform);
    assert_eq!(posture.secret_ref(), None);
}

#[test]
fn a_self_managed_row_without_a_credential_is_reported_not_downgraded() {
    // The failure that matters: silently reading this as platform mode would
    // dial the shared key for a tenant that asked for its own.
    assert_eq!(
        Posture::from_columns(MODE_SELF_MANAGED, None),
        Err(StoredPosture::SelfManagedWithoutCredential)
    );
}

#[test]
fn a_self_managed_row_with_an_unreadable_credential_keeps_the_reason() {
    assert_eq!(
        Posture::from_columns(MODE_SELF_MANAGED, Some("  ")),
        Err(StoredPosture::Credential(MalformedSecretRef::Blank))
    );
}

#[test]
fn a_mode_this_daemon_does_not_serve_names_itself_in_the_refusal() {
    let Err(StoredPosture::UnknownMode { mode }) = Posture::from_columns("byo_gateway", None)
    else {
        panic!("an unknown mode must not read as a posture");
    };
    // The spelling is carried so an operator repairing the row can see what is
    // in it without querying separately.
    assert_eq!(&*mode, "byo_gateway");
}

#[test]
fn a_posture_round_trips_through_the_two_columns_it_was_read_from() {
    for posture in [
        Posture::Platform,
        Posture::SelfManaged {
            secret_ref: SecretRef::parse(A_KEY).expect("a well-formed name"),
        },
    ] {
        let round_tripped = Posture::from_columns(posture.mode(), posture.secret_ref())
            .expect("a posture's own columns are readable");
        assert_eq!(round_tripped, posture);
    }
}

#[test]
fn the_two_modes_do_not_share_a_spelling() {
    // A single `mode()` returning the same string for both would make every
    // stored row platform mode, and every test above would still pass.
    assert_ne!(Posture::Platform.mode(), MODE_SELF_MANAGED);
    assert_eq!(Posture::Platform.mode(), MODE_PLATFORM);
}
