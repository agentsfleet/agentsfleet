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

use af_core::id::{ENTROPY_LEN, Uuid7};
use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use afd_billing::Accounts;
use afd_credential::credential::platform::Platform;
use afd_credential::credential::{Broker, Vendors};
use afd_credential::provider::Providers;
use afd_credential::secrets::Registry;
use afd_credential::vault::Vault;
use afd_db::Db;
use afd_gate::gate::Gates;
use jiff::Timestamp;

use crate::lease::affinity::Fence;
use crate::lease::envelope::{Acquired, Kind};
use crate::lease::pull::LeaseService;
use crate::lease::store::Leases;
use afd_memory::Memories;

/// The instant every fixture here is stamped at.
pub(crate) const AT: Timestamp = Timestamp::constant(1_767_225_600, 0);

/// A pool whose every acquire fails as the transport class.
pub(crate) fn database() -> Db {
    afd_db::test_util::unreachable_db()
}

/// A bus whose every publish and subscribe fails.
pub(crate) fn bus() -> afd_bus::Handle {
    afd_bus::test_util::unreachable()
}

/// A lease store over both.
pub(crate) fn leases() -> Leases {
    Leases::new(database(), bus(), Entropy::new())
}

/// The whole lease plane over both.
#[expect(
    clippy::disallowed_methods,
    reason = "the broker's vendors hold Platform::empty(), so this client never exchanges a credential or sends a request; afd_identity::direct's deadline and no_proxy have nothing to bound"
)]
pub(crate) fn plane() -> LeaseService {
    let kek = Arc::new(Kek::from_bytes([7; af_crypto::KEY_LEN]));
    LeaseService {
        task_reports: None,
        leases: leases(),
        gates: Gates::new(database(), bus(), Entropy::new()),
        accounts: Accounts::new(database()),
        biller: Arc::new(biller()),
        memories: Memories::new(database(), Entropy::new()),
        providers: Providers::new(database(), Arc::clone(&kek), Entropy::new()),
        vault: Vault::new(database(), kek),
        broker: Arc::new(Broker::new(
            Arc::new(Registry::default()),
            Arc::new(Vendors::new(Platform::empty(), reqwest::Client::new())),
        )),
        grants: afd_approval::IntegrationGrants::new(database(), Entropy::new()),
        thread: Arc::new(afd_events::History::new(database())),
    }
}

/// The built-in biller over the dead pool.
pub(crate) fn biller() -> afd_billing::ledger::LedgerBiller {
    afd_billing::ledger::LedgerBiller::new(
        database(),
        Entropy::new(),
        Arc::new(af_core::clock::SystemClock),
    )
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
        leased_until: af_core::clock::after(AT, af_core::timing::LEASE_TTL),
        kind: Kind::Fresh,
        event_id: id(5),
        actor: "fixture:steer".to_owned(),
        event_type: "chat".to_owned(),
        request_json: "{}".to_owned(),
        workspace_id: id(2),
        event_created_at: AT,
        reused: None,
        resume_hold: false,
        redelivered: false,
        opened: crate::lease::envelope::Opened::unread(id(2), id(3)),
    }
}
