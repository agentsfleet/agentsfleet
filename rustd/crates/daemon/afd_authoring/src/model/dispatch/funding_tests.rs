//! The hold's input term and the counting-key fence, apart from the database.

use super::{input_hold, same_counting_key};
use crate::Result;
use afd_core::error_code;
use afd_credential::provider::RegisteredModel;
use afd_crypto::secret::SecretString;
use serde_json::json;

/// Three dollars per million input tokens, in nanos.
const INPUT_PRICE: i64 = 3_000_000_000;

fn model(platform_managed: bool) -> Result<RegisteredModel> {
    serde_json::from_value(json!({
        "reference":"platform-default","provider":"openai","model":"gpt-5.6-terra",
        "inference":{"api":"responses","base_url":"https://api.openai.com/v1","max_output_tokens":8192},
        "context_window_tokens":400_000,"input_nanos_per_million":INPUT_PRICE,
        "output_nanos_per_million":0,"resolution_version":"fixture",
        "platform_managed":platform_managed
    }))
    .map_err(crate::error::raise::stored)
}

fn key(value: &str) -> SecretString {
    SecretString::new(value.to_owned())
}

#[test]
fn a_tenant_paid_model_holds_nothing_for_its_input() -> Result<()> {
    assert_eq!(input_hold(&model(false)?, 1_000)?, 0);
    Ok(())
}

#[test]
fn a_platform_paid_model_holds_its_measured_input_at_the_model_price() -> Result<()> {
    // 1,000 tokens at $3 per million is 3,000,000 nanos; one token rounds up.
    assert_eq!(input_hold(&model(true)?, 1_000)?, 3_000_000);
    assert_eq!(input_hold(&model(true)?, 1)?, 3_000);
    Ok(())
}

#[test]
fn an_input_too_large_to_price_refuses_the_hold() -> Result<()> {
    // No hold covers it, so the call is refused rather than under-reserved.
    let refused = input_hold(&model(true)?, u64::MAX).err();
    assert_eq!(
        refused.map(|error| error.code()),
        Some(error_code::AUTHORING_BALANCE)
    );
    Ok(())
}

#[test]
fn a_request_sent_without_a_native_count_needs_no_key_match() -> Result<()> {
    same_counting_key(None, &key("sk-current"))
}

#[test]
fn the_key_the_count_ran_under_may_send() -> Result<()> {
    same_counting_key(Some(&key("sk-current")), &key("sk-current"))
}

#[test]
fn a_key_rotated_after_the_count_is_refused() {
    // The count does not vouch for another credential.
    let refused = same_counting_key(Some(&key("sk-counted")), &key("sk-rotated")).err();
    assert_eq!(
        refused.map(|error| error.code()),
        Some(error_code::AUTHORING_FENCE)
    );
}
