//! What every device-flow suite builds its logins from: the login surface over
//! the lane's Postgres, and the one approval, code and caller they all present.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use af_crypto::entropy::Entropy;
use af_crypto::secret::SecretBytes;
use afd_tenant::session::input::Approval;
use afd_tenant::session::{DeviceSessions, Fingerprint, Sessions};
use jiff::Timestamp;

use crate::session_harness::SessionHarness;

/// The dashboard this suite's login URLs are built against.
pub(crate) const APP_URL: &str = "https://app-dev.agentsfleet.net/";

/// The digits a person reads out of the browser in these tests.
pub(crate) const CODE: &str = "428193";

/// The identity that approves them.
pub(crate) const APPROVER: &str = "user_2abcDEF";

/// A fixed instant for the session identifier `open` mints, so nothing here
/// depends on when it runs. The row's deadlines are the database clock's.
pub(crate) const NOW: Timestamp = Timestamp::constant(1_700_000_000, 0);

/// The login surface, over the lane's database.
pub(crate) fn surface(harness: &SessionHarness) -> Sessions {
    Sessions::new(
        DeviceSessions::new(harness.database.clone()),
        SecretBytes::new(b"c0ffee".repeat(8)),
        Entropy::new(),
        APP_URL,
    )
}

/// An approval carrying this suite's fixed envelope.
pub(crate) fn approval() -> Approval<'static> {
    Approval::parse("dash-key", "sealed-credential", "nonce-12", CODE).expect("bounds")
}

/// An approver no other test shares, for a test that bulk-cancels.
///
/// `cancel_all` aborts every live login its owner approved, and the suites run
/// in parallel against one database. A test cancelling as [`APPROVER`] would abort
/// the logins a sibling test is still driving, so each canceller takes its own
/// owner; the process id keeps two concurrent runs on one database apart too.
pub(crate) fn owner(test: &str) -> String {
    format!("{APPROVER}_{test}_{}", std::process::id())
}

/// A caller's identity for the replay window.
pub(crate) fn origin(session_id: &str) -> Fingerprint {
    Fingerprint::of("203.0.113.7", "agentsfleet/1.0", session_id)
}
