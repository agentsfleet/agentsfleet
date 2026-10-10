//! What a credential lookup says when the pool gives it no connection.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_auth::credential::CredentialKind;
use afd_auth::directory::{CredentialDirectory as _, Digest};
use afd_core::test_util::trace::Capture;

use super::Credentials;
use crate::error::Unavailable;

/// A lookup with no connection answers unavailable, and its line says why
/// without repeating the code the line already carries, or a backtrace.
#[tokio::test]
async fn an_unavailable_credential_lookup_reason_carries_no_code_prefix() {
    let capture = Capture::install();
    let credentials = Credentials::new(afd_db::test_util::unreachable_db());

    let answered = credentials
        .resolve(CredentialKind::TenantApiKey, &Digest::of_minted("agt_t0"))
        .await;

    assert_eq!(answered.err(), Some(Unavailable));
    let logged = capture.only("credential_lookup_unavailable");
    let reason = logged.field("reason").expect("the line says why");
    assert!(!reason.is_empty() && !reason.contains("[UZ-") && !reason.contains('\n'));
    assert_eq!(logged.field("error_code"), Some("UZ-INTERNAL-001"));
}
