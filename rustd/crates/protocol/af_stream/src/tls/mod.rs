//! Verified TLS configuration shared by both runner transports.

mod boring;
pub(crate) mod key_exchange;

pub use boring::{Identity, Trust};
pub use key_exchange::KeyExchange;
