#![expect(
    clippy::expect_used,
    reason = "the success arm is the fixture precondition"
)]

use zeroize::Zeroizing;

use super::{Minted, Outcome, Retry};

#[test]
fn only_a_success_can_expose_a_minted_token() {
    let success = Outcome::Ok(Minted {
        token: Zeroizing::new("secret-token".to_owned()),
        expires_at_ms: 42,
        rotated_refresh_token: Some(Zeroizing::new("new-refresh".to_owned())),
    });
    let minted = success
        .minted()
        .expect("the success carries its credential");
    assert_eq!(minted.token.as_str(), "secret-token");
    assert_eq!(minted.expires_at_ms, 42);
    assert!(format!("{minted:?}").contains("rotated: true"));
    assert!(!format!("{minted:?}").contains("secret-token"));

    for refusal in [
        Outcome::ReconnectRequired,
        Outcome::MintFailed(Retry::Transient),
        Outcome::MintFailed(Retry::Permanent),
        Outcome::Unconfigured,
        Outcome::UnknownIntegration,
    ] {
        assert!(refusal.minted().is_none());
    }
}
