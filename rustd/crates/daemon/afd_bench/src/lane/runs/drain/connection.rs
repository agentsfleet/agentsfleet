//! Compose the same gateway handles as daemon boot, before starting measured windows.

use super::stage;
use crate::lane::runs::seed::RunnerIdentity;
use crate::{datastores::Datastores, error::Result};
use af_crypto::{entropy::Entropy, secret::Kek};
use af_wire::run::AcceptedKinds;
use afd_gateway::{Gateway, GatewayDependencies, test_util::RunnerConnection};
use afd_setup::{Creations, Drafts, Setup};
use std::sync::Arc;

pub(in crate::lane::runs) fn gateway(stores: &Datastores, kek: &Arc<Kek>) -> (Gateway, Drafts) {
    let runs = stage::plane(stores, kek);
    let drafts = Drafts::new(stores.database.clone(), Entropy::new());
    let turns = afd_turns::Turns::new(stores.database.clone(), stores.bus.clone(), Entropy::new());
    let creations = Creations::new(Setup::new(drafts.clone(), Arc::clone(kek)), turns);
    let gateway = Gateway::new(GatewayDependencies {
        recovery: afd_gateway::RecoverySettings::default(),
        database: stores.database.clone(),
        runners: afd_runner::Runners::new(stores.database.clone(), Entropy::new()),
        runs,
        drafts: drafts.clone(),
        creations,
        bus: stores.bus.clone(),
        stream_limit: afd_sse::StreamLimit::new(usize::MAX),
        wake: afd_bus::wake::Wake::default(),
        frame_memory: af_stream::MemoryLimits::default(),
        wall_clock: Arc::new(af_core::clock::SystemClock),
        analytics: af_observability::Analytics::silent(),
    });
    (gateway, drafts)
}

pub(in crate::lane::runs) async fn open(
    gateway: &Gateway,
    runner: &RunnerIdentity,
    kinds: AcceptedKinds,
    credits: usize,
) -> Result<RunnerConnection> {
    Ok(RunnerConnection::open(
        gateway,
        runner.id.clone(),
        runner.digest.clone(),
        kinds,
        credits,
    )
    .await?)
}
