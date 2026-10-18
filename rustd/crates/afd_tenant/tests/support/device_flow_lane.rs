//! What every device-flow suite builds its logins from: the login surface over
//! the lane's queue, and the one approval, code and caller they all present.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_crypto::entropy::Entropy;
use afd_crypto::secret::SecretBytes;
use afd_dragonfly::SessionStore;
use afd_tenant::session::input::Approval;
use afd_tenant::session::{Fingerprint, Sessions};
use jiff::Timestamp;

use crate::dragonfly_harness::DragonflyHarness;

/// The dashboard this suite's login URLs are built against.
pub(crate) const APP_URL: &str = "https://app-dev.agentsfleet.net/";

/// The digits a person reads out of the browser in these tests.
pub(crate) const CODE: &str = "428193";

/// The identity that approves them.
pub(crate) const APPROVER: &str = "user_2abcDEF";

/// A fixed instant, so nothing here depends on when it runs. The session
/// lives in Dragonfly, so the daemon's clock stamps it, and this stands in.
const NOW: Timestamp = Timestamp::constant(1_700_000_000, 0);

/// The login surface, over the lane's queue.
pub(crate) fn surface(harness: &DragonflyHarness) -> Sessions {
    Sessions::new(
        SessionStore::new(harness.redis.clone()),
        SecretBytes::new(b"c0ffee".repeat(8)),
        Entropy::new(),
        APP_URL,
    )
}

/// The instant every write in one of these tests is stamped with.
pub(crate) const fn now() -> Timestamp {
    NOW
}

/// An approval carrying this suite's fixed envelope.
pub(crate) fn approval() -> Approval<'static> {
    Approval::parse("dash-key", "sealed-credential", "nonce-12", CODE).expect("bounds")
}

/// A caller's identity for the replay window.
pub(crate) fn origin(session_id: &str) -> Fingerprint {
    Fingerprint::of("203.0.113.7", "agentsfleet/1.0", session_id)
}
