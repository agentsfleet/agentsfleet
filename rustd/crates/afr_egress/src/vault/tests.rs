use afd_core::clock::{FixedClock, UnixMillis};

use super::{REFRESH_MARGIN_MILLIS, Vault};
use crate::admission::Admission;
use crate::fixture::{GRAFANA_TOKEN, policy};
use crate::refusal::Refusal;
use crate::testing::CountingMint;

/// When every suite's clock starts.
const START: UnixMillis = UnixMillis::from_millis(1_700_000_000_000);
/// A minted token's lifetime: one hour.
const HOUR: i64 = 3_600_000;
/// The token the mint answers.
const MINTED: &str = "ghs_minted_token";

#[tokio::test]
async fn should_fill_a_static_placeholder_from_secrets_map() {
    let policy = policy(false);
    let clock = FixedClock::at(START);
    let mint = CountingMint::answering(MINTED, HOUR, clock.clone());
    let mut vault = Vault::new(&mint, &clock);

    let filled = vault
        .fill(Admission::new(&policy), "Bearer ${secrets.grafana.token}")
        .await;

    assert_eq!(
        filled.map(|secret| secret.expose().to_owned()),
        Ok(format!("Bearer {GRAFANA_TOKEN}"))
    );
    assert_eq!(mint.asked(), 0);
}

#[tokio::test]
async fn should_mint_once_and_reuse_until_shortly_before_expiry() {
    let policy = policy(false);
    let admission = Admission::new(&policy);
    let clock = FixedClock::at(START);
    let mint = CountingMint::answering(MINTED, HOUR, clock.clone());
    let mut vault = Vault::new(&mint, &clock);

    for _call in 0..3 {
        let filled = vault.fill(admission, "token ${secrets.github.token}").await;
        assert_eq!(
            filled.map(|secret| secret.expose().to_owned()),
            Ok(format!("token {MINTED}"))
        );
    }
    assert_eq!(mint.asked(), 1);

    clock.advance_millis(HOUR - REFRESH_MARGIN_MILLIS);
    let refilled = vault.fill(admission, "token ${secrets.github.token}").await;

    assert!(refilled.is_ok_and(|secret| secret.expose().ends_with(MINTED)));
    assert_eq!(mint.asked(), 2);
}

#[tokio::test]
async fn should_refuse_with_the_daemons_words_when_the_mint_is_refused() {
    let policy = policy(false);
    let clock = FixedClock::at(START);
    let mint = CountingMint::refusing("UZ-REPAIR-004: grant revoked", clock.clone());
    let mut vault = Vault::new(&mint, &clock);

    let filled = vault
        .fill(Admission::new(&policy), "${secrets.github.token}")
        .await;

    assert_eq!(
        filled.err(),
        Some(Refusal::CredentialMintRefused {
            detail: "UZ-REPAIR-004: grant revoked".to_owned()
        })
    );
}

#[tokio::test]
async fn should_mask_every_minted_token_and_nothing_before_a_mint() {
    let policy = policy(false);
    let clock = FixedClock::at(START);
    let mint = CountingMint::answering(MINTED, HOUR, clock.clone());
    let mut vault = Vault::new(&mint, &clock);
    let echoed = format!("upstream echoed {MINTED}");

    assert_eq!(vault.mask(&echoed), echoed);
    let filled = vault
        .fill(Admission::new(&policy), "${secrets.github.token}")
        .await;

    assert_eq!(
        filled.map(|secret| secret.expose().to_owned()),
        Ok(MINTED.to_owned())
    );
    assert_eq!(vault.mask(&echoed), "upstream echoed «secret:github.token»");
}
