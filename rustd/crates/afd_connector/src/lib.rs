//! Connecting a workspace to a third party: the signed install state, the
//! code-for-token exchange, and the grant that lands in the vault.
//!
//! ```text
//!   person ──► state::sign        workspace + who started it, signed
//!                │                nonce::remember — the single-use slot
//!                ▼
//!              oauth::authorize_url ──► the provider's consent screen
//!                │
//!                ▼  (the browser comes back through the dashboard)
//!              state::verify      genuine, unexpired, and theirs
//!                │                nonce::consume — spent, exactly once
//!                ▼
//!              exchange::redeem   the code, at the provider that issued it
//!                │
//!                ▼
//!              grant::parse       the provider's answer as a handle
//!                │
//!                ▼
//!              grant::land        sealed in the vault, routed back
//! ```
//!
//! # What this crate is, and firmly is not
//!
//! It owns the state machine and the vendor conversation. It owns no routing,
//! no HTTP surface and no extractor: whether a caller MAY connect this
//! workspace is decided at the edge, in `afd_api`, by a layer mounted from the
//! route's own template. This answers what a connect DOES once that decision is
//! made.
//!
//! It also verifies no INBOUND signature. A Slack delivery is proven by
//! `afd_webhook::Scheme::SlackV0` against the secret [`app::PlatformApp`] reads
//! — the wall is one crate for every surface that has one, and folding a second
//! copy in here is exactly the duplication RULE OWN exists to prevent.
//!
//! # Two archetypes, five providers, one route family
//!
//! [`provider::Provider`] is a closed enum and [`registry::Archetype`] is a
//! closed enum over it, so adding a connector is an arm in each match plus its
//! grant parse — never a new route and never new flow code. `registry.zig`
//! makes the same claim and enforces it at `comptime`; here it is the
//! language's, one build stage earlier.
//!
//! # The order is the security property
//!
//! Verify the state, check it against the person who is presenting it,
//! re-authorise the workspace, and only THEN consume the nonce. Consuming
//! first lets any authenticated person burn somebody else's in-flight connect
//! by replaying its callback URL — see [`state`] on why the two steps are
//! separate functions rather than one.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit. Gated on `not(test)` because the test build links
// dev-dependencies into this same target.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

mod complete;

pub mod app;
pub mod callback;
pub mod connect;
pub mod connection;
pub mod error;
pub mod exchange;
pub mod grant;
pub mod jira;
pub mod oauth;
pub mod provider;
pub mod registry;
pub mod sql;
pub mod state;
pub mod zoho;

pub use self::app::PlatformApp;
pub use self::callback::Handoff;
pub use self::complete::{Finishing, Landed};
pub use self::connect::{Connectors, Spent, Started, Starting};
pub use self::connection::Catalogued;
pub use self::error::{Error, Result};
pub use self::exchange::{AppCredentials, Exchange, Exchanged};
pub use self::grant::{Connection, Forgotten, Grant, Grants, Install};
pub use self::provider::Provider;
pub use self::registry::{AppInstall, Archetype, Oauth2Flow, STATE_TTL_SECONDS, StateBinding};
pub use self::state::{Rejected, Verified};
