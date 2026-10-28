//! Connector readiness refuses unusable stored handles without a provider call.

mod binding;
mod oauth;
mod slack;

use super::Handle;
use crate::secrets::{Exchange, Supply};

fn usable(raw: &str, name: &str, supply: Supply, exchange: Exchange) -> bool {
    afd_core::json::object_from_slice::<Handle<'_>>(raw.as_bytes())
        .is_ok_and(|handle| handle.usable(name, supply, Some(exchange)))
}

#[test]
fn svix_readiness_uses_the_same_named_signing_field_as_ingress() {
    let secret = format!("whsec_{}", "YQ==");
    for (raw, accepted) in [
        (
            serde_json::json!({"webhook_secret":secret}).to_string(),
            true,
        ),
        (
            serde_json::json!({"webhook_secret":"not-svix"}).to_string(),
            false,
        ),
        (serde_json::json!({"webhook_secret":""}).to_string(), false),
        (serde_json::json!({"webhook_secret":42}).to_string(), false),
        (serde_json::json!({"secret":secret}).to_string(), false),
    ] {
        let parsed = afd_core::json::object_from_slice::<Handle<'_>>(raw.as_bytes());
        assert_eq!(
            parsed.map(|handle| handle.svix_usable()).ok(),
            Some(accepted)
        );
    }
}
