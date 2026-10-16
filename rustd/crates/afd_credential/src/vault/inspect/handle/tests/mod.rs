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
