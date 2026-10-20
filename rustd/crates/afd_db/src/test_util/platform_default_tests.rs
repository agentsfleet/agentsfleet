//! The platform-default hold, when its removal cannot run.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the library"
)]

use afd_core::error_code::{self, Coded as _};
use afd_core::test_util::trace::Capture;

use super::{
    Created, DELETE_POLL, NOT_REMOVED, PlatformDefault, REMOVAL_PANICKED, Unremoved, remove,
    states, unremoved,
};

/// A Postgres nobody listens on: port 1 is reserved and unbound.
const NOWHERE: &str = "postgres://fixture:fixture@127.0.0.1:1/fixture";

/// A row this process recorded as its own, in a database that is gone.
fn stranded() -> Created {
    Created {
        url: NOWHERE.to_owned(),
        workspace: "0199f0c0-0000-7000-8000-000000000002".to_owned(),
    }
}

/// A removal that cannot reach its database fails as the outage it is,
/// named for the removal's own connection.
#[test]
fn a_removal_with_no_database_fails_as_an_outage() {
    let failure = remove("stranded-provider", &stranded()).expect_err("nothing listens");

    assert!(failure.is_datastore_unavailable(), "{failure}");
    assert_eq!(failure.code(), error_code::INTERNAL_DB_UNAVAILABLE);
}

/// The last hold over a row it cannot remove says so, and leaves the
/// provider free for the next holder rather than stuck mid-delete.
#[tokio::test]
async fn the_last_hold_over_an_unremovable_row_logs_it_and_frees_the_provider() {
    const PROVIDER: &str = "stranded-hold-provider";
    let hold = PlatformDefault::enter(PROVIDER).await;
    states().entry(PROVIDER.to_owned()).or_default().created = Some(stranded());
    let capture = Capture::install();

    drop(hold);

    let line = capture.only(NOT_REMOVED);
    assert_eq!(
        line.field("error_code"),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    line.assert_uncoded_reason();
    let state = states();
    let left = state.get(PROVIDER).expect("the provider was counted");
    assert_eq!((left.holders, left.deleting), (0, false));
    assert!(left.created.is_none(), "the drop took the row it tried");
}

/// A holder arriving while the provider's row is being deleted waits for the
/// delete to finish, then holds: it never counts itself onto a row going away.
#[tokio::test]
async fn a_holder_waits_out_a_delete_then_holds() {
    const PROVIDER: &str = "deleting-provider";
    states().entry(PROVIDER.to_owned()).or_default().deleting = true;
    let mut arriving = tokio::spawn(PlatformDefault::enter(PROVIDER));

    let waited = tokio::time::timeout(DELETE_POLL * 5, &mut arriving).await;
    assert!(waited.is_err(), "a holder does not enter mid-delete");
    assert_eq!(states().get(PROVIDER).map(|state| state.holders), Some(0));

    states().entry(PROVIDER.to_owned()).or_default().deleting = false;
    let hold = arriving.await.expect("the holder's task completes");
    assert_eq!(states().get(PROVIDER).map(|state| state.holders), Some(1));
    drop(hold);
    assert_eq!(states().get(PROVIDER).map(|state| state.holders), Some(0));
}

#[test]
fn a_leftover_platform_default_reason_carries_no_code_prefix() {
    assert_eq!(
        unremoved(Ok(Ok(()))),
        None,
        "a removed row leaves nothing to say"
    );
    for (kind, failure) in crate::error::one_of_each_kind() {
        let logged = failure.logged();
        afd_core::test_util::trace::assert_uncoded(&logged.reason, kind);
        assert_eq!(
            unremoved(Ok(Err(failure))),
            Some(Unremoved::Failed(logged)),
            "{kind}: the line keeps the failure's code beside its sentence"
        );
    }
    assert_eq!(unremoved(Err(Box::new("boom"))), Some(Unremoved::Panicked));
}

/// A failed removal's line carries its code; a panicked one, with no
/// failure to name, carries none rather than borrowing an outage's.
#[test]
fn a_panicked_removal_is_logged_without_a_code() {
    assert_eq!(
        Unremoved::Panicked.line(),
        (None, REMOVAL_PANICKED.to_owned())
    );
    let (_kind, failure) = crate::error::one_of_each_kind()
        .into_iter()
        .next()
        .expect("the crate has an error kind");
    let logged = failure.logged();
    assert_eq!(
        Unremoved::Failed(logged.clone()).line(),
        (Some(logged.error_code), logged.reason)
    );
}
