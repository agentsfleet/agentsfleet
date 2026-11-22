//! A gateway composed the way boot composes it, over the datastore and broker
//! a test hands it.

use std::sync::Arc;

use af_crypto::{entropy::Entropy, secret::Kek};
use afd_credential::credential::{Broker, Vendors, platform::Platform};
use afd_fleet::run::{RunService, Runs};

use crate::{FrameCeilings, Gateway, GatewayDependencies, RecoverySettings};

/// The key every sealed fixture value uses; no test reads anything it seals.
const KEY: [u8; 32] = [0x11; 32];

/// A gateway over `database` and `bus`, its scheduler waking on `wake`.
pub(crate) fn gateway(
    database: afd_db::Db,
    bus: afd_bus::Handle,
    wake: afd_bus::wake::Wake,
) -> Result<Gateway, Box<dyn std::error::Error + Send + Sync>> {
    let key = Arc::new(Kek::from_bytes(KEY));
    let vendors = afd_identity::direct(af_core::timing::STREAM_ANSWER_TIMEOUT).build()?;
    let runs = RunService {
        durable_task_reports: None,
        turn_checkpoints: afd_fleet::run::turn_checkpoint::TurnCheckpoints::new(Arc::clone(&key)),
        runs: Runs::new(database.clone(), bus.clone(), Entropy::new()),
        gates: afd_gate::gate::Gates::new(database.clone(), bus.clone(), Entropy::new()),
        accounts: afd_billing::Accounts::new(database.clone()),
        biller: Arc::new(afd_billing::ledger::LedgerBiller::new(database.clone())),
        memories: afd_memory::Memories::new(database.clone(), Entropy::new()),
        providers: afd_credential::provider::Providers::new(
            database.clone(),
            Arc::clone(&key),
            Entropy::new(),
        ),
        vault: afd_vault::Vault::new(database.clone(), Arc::clone(&key), Entropy::new()),
        broker: Arc::new(Broker::new(
            Arc::new(afd_credential::secrets::Registry::default()),
            Arc::new(Vendors::new(Platform::empty(), vendors)),
        )),
        grants: afd_approval::IntegrationGrants::new(database.clone(), Entropy::new()),
        thread: Arc::new(afd_fleet::run::history::Conversations::new(
            database.clone(),
        )),
    };
    let drafts = afd_setup::Drafts::new(database.clone(), Entropy::new()).with_bus(bus.clone());
    let turns = afd_turns::Turns::new(database.clone(), bus.clone(), Entropy::new());
    let creations = afd_setup::Creations::new(
        afd_setup::Setup::new(drafts.clone(), Arc::clone(&key)),
        turns,
    );
    Ok(Gateway::new(GatewayDependencies {
        recovery: RecoverySettings::default(),
        runners: afd_runner::Runners::new(database.clone(), Entropy::new()),
        database,
        runs,
        drafts,
        creations,
        bus,
        stream_limit: afd_sse::StreamLimit::new(usize::MAX),
        wake,
        frame_memory: af_stream::MemoryLimits::default(),
        wall_clock: Arc::new(af_core::clock::SystemClock),
        analytics: af_observability::Analytics::silent(),
        frames: FrameCeilings::default(),
    }))
}

/// What every [`Silent`] call answers: nothing, ever.
type Never<'a, T> = std::pin::Pin<Box<dyn Future<Output = afd_bus::Result<T>> + Send + 'a>>;

/// A broker that took the connection and never answers it: every publish and
/// subscribe waits forever, so only the caller's own deadline ends the wait.
///
/// Written out rather than through `#[async_trait]`, which this crate does not
/// depend on; the signatures are what that macro expands to.
#[derive(Debug)]
pub(crate) struct Silent;

impl afd_bus::Bus for Silent {
    fn publish<'s, 'a, 'b, 'f>(
        &'s self,
        _subject: &'a afd_bus::Subject,
        _payload: &'b str,
    ) -> Never<'f, ()>
    where
        's: 'f,
        'a: 'f,
        'b: 'f,
        Self: 'f,
    {
        Box::pin(std::future::pending())
    }

    fn subscribe<'s, 'a, 'f>(
        &'s self,
        _subject: &'a afd_bus::Subject,
    ) -> Never<'f, afd_bus::Subscription>
    where
        's: 'f,
        'a: 'f,
        Self: 'f,
    {
        Box::pin(std::future::pending())
    }

    fn subscribe_thread_wakes<'s, 'f>(&'s self) -> Never<'f, afd_bus::Subscription>
    where
        's: 'f,
        Self: 'f,
    {
        Box::pin(std::future::pending())
    }

    fn shutdown<'s, 'f>(&'s self) -> Never<'f, ()>
    where
        's: 'f,
        Self: 'f,
    {
        Box::pin(std::future::ready(Ok(())))
    }
}
