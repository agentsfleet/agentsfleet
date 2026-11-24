//! What a `hello` says when the counters do not answer.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;

use super::{EVENT_HELLO_COUNTERS_UNREAD, counters_unread};

/// A `hello` whose counters did not answer says why without repeating its
/// code, which the line's own `error_code` field already carries.
#[test]
fn an_unread_hello_counter_reason_carries_no_code_prefix() {
    let capture = Capture::install();
    let workspace =
        Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011").expect("a fixture identifier");
    let refused = afd_db::PoolConfig::resolve(
        &af_core::env::MapEnv::default(),
        afd_db::config::DbRole::Api,
    )
    .expect_err("no database is configured");
    let failure = afd_fleet_lifecycle::Error::from(refused);

    counters_unread(&workspace, &failure);

    let logged = capture.only(EVENT_HELLO_COUNTERS_UNREAD);
    logged.assert_uncoded_reason();
    assert_eq!(logged.field("error_code"), Some(failure.code().as_str()));
}
