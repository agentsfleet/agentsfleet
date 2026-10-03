//! The runner's outbound guard.
//!
//! A supervisor-side tool hands its request to the lease's [`Egress`] as a
//! [`Draft`]. It is admitted against the lease's network policy and the
//! daemon's origin rules before any connection exists; its
//! `${secrets.NAME.FIELD}` placeholders are put in place at send time, from the
//! lease's static secrets or a token minted once per lease; and it leaves
//! through a [`Transport`] whose resolver never answers a private address. Only
//! this crate builds an [`Outbound`], so nothing reaches a transport without
//! admission. What comes back is masked for every minted token before a tool
//! reads it (`docs/architecture/runner_fleet.md` §"Egress model — outbound is
//! the only network surface").

pub mod error;
#[cfg(any(test, feature = "test-util"))]
pub mod fixture;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;

mod admission;
mod egress;
mod mint;
mod network;
mod origin;
mod placeholder;
mod refusal;
mod transport;
mod vault;

pub use self::admission::{Draft, Placement};
pub use self::egress::Egress;
pub use self::error::{Error, Result};
pub use self::mint::{Mint, MintRefused, Minted};
pub use self::network::{Network, RESPONSE_MAX_BYTES};
pub use self::refusal::Refusal;
pub use self::transport::{Inbound, Outbound, Transport};
