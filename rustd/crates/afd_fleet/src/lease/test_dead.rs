//! A lease plane over datastores nobody answers on, for the unit tests that
//! prove what each failure arm does.
//!
//! Every handle is lazy and opens no socket until asked, so a test gets the
//! refusal a real outage produces without a datastore anywhere near it.

#![expect(
    clippy::expect_used,
    reason = "a fixture whose own constants are malformed should stop the suite"
)]

use std::sync::Arc;

use afd_billing::Accounts;
use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_credential::credential::platform::Platform;
use afd_credential::credential::{Broker, Vendors};
use afd_credential::provider::Providers;
use afd_credential::secrets::Registry;
use afd_credential::vault::Vault;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_dragonfly::{Dragonfly, DragonflyConfig, DragonflyRole, EventId, ReadyToken};
use afd_gate::gate::Gates;

use crate::lease::affinity::Fence;
use crate::lease::envelope::{Acquired, Kind};
use crate::lease::pull::Plane;
use crate::lease::store::Leases;
use afd_memory::Memories;

/// A Dragonfly nobody listens on: port 1 is reserved and unbound.
const NOWHERE_QUEUE: &str = "redis://127.0.0.1:1";

/// The instant every fixture here is stamped at.
pub(crate) const AT: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

/// A pool whose every acquire fails as the transport class.
pub(crate) fn database() -> Db {
    afd_db::test_util::unreachable_db()
}

/// A queue whose every command fails on connection refusal.
pub(crate) fn queue() -> Dragonfly {
    let config = DragonflyConfig::from_url(DragonflyRole::Default, NOWHERE_QUEUE.to_owned());
    Dragonfly::unreachable(&config).expect("a lazy manager opens no socket")
}

/// A lease store over both.
pub(crate) fn leases() -> Leases {
    Leases::new(database(), queue(), Entropy::new())
}

/// The whole lease plane over both.
pub(crate) fn plane() -> Plane {
    let kek = Arc::new(Kek::from_bytes([7; afd_crypto::KEY_LEN]));
    Plane {
        leases: leases(),
        gates: Gates::new(database(), queue(), Entropy::new()),
        accounts: Accounts::new(database(), Entropy::new()),
        memories: Memories::new(database(), Entropy::new()),
        providers: Providers::new(database(), Arc::clone(&kek), Entropy::new()),
        vault: Vault::new(database(), kek),
        broker: Arc::new(Broker::new(
            Arc::new(Registry::default()),
            Arc::new(Vendors::new(Platform::empty(), reqwest::Client::new())),
        )),
        grants: afd_approval::IntegrationGrants::new(database(), Entropy::new()),
        thread: Arc::new(afd_events::History::new(database())),
        connectors: Registry::default(),
    }
}

/// A fixed identifier, distinct per `seed`.
pub(crate) fn id(seed: u8) -> Uuid7 {
    Uuid7::encode(AT, [seed; ENTROPY_LEN]).expect("a fixed instant encodes")
}

/// A claimed fresh event on fleet `id(1)`, as a poll would have handed it.
pub(crate) fn acquired() -> Acquired {
    Acquired {
        fleet_id: id(1),
        fence: Fence::from_i64(3),
        leased_until: AT.saturating_add_millis(30_000),
        kind: Kind::Fresh,
        event_id: "1767225600000-1".to_owned(),
        receipt: EventId::of("1767225600000-0"),
        actor: "fixture:steer".to_owned(),
        event_type: "chat".to_owned(),
        request_json: "{}".to_owned(),
        workspace_id: id(2),
        event_created_at: AT,
        reused: None,
        ready: ReadyToken::mint(&Entropy::new(), AT).expect("the host has entropy"),
    }
}
