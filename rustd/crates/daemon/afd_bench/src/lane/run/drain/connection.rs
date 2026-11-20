//! Compose the same gateway handles as daemon boot, before starting measured windows.

use super::stage;
use crate::lane::run::seed::RunnerIdentity;
use crate::{datastores::Datastores, error::Result};
use af_crypto::{entropy::Entropy, secret::Kek};
use af_wire::run::AcceptedKinds;
use afd_gateway::{Gateway, GatewayDependencies, test_util::RunnerConnection};
use afd_setup::{Creations, Drafts, ModelCalls, Network, Setup};
use std::sync::Arc;

pub(in crate::lane::run) fn gateway(
    stores: &Datastores,
    kek: &Arc<Kek>,
) -> Result<(Gateway, Drafts)> {
    let runs = stage::plane(stores, kek);
    let drafts = Drafts::new(
        stores.database.clone(),
        Entropy::new(),
        Arc::clone(&runs.biller),
    );
    let models = ModelCalls::new(
        drafts.clone(),
        runs.providers.clone(),
        Arc::new(Network::new()?),
    );
    let turns = afd_turns::Turns::new(stores.database.clone(), stores.bus.clone(), Entropy::new());
    let creations = Creations::new(Setup::new(drafts.clone(), Arc::clone(kek)), turns);
    let gateway = Gateway::new(GatewayDependencies {
        recovery: afd_gateway::RecoverySettings::default(),
        database: stores.database.clone(),
        runners: afd_runner::Runners::new(stores.database.clone(), Entropy::new()),
        runs,
        drafts: drafts.clone(),
        models,
        creations,
        bus: stores.bus.clone(),
        stream_limit: afd_sse::StreamLimit::new(usize::MAX),
        wake: afd_bus::wake::Wake::default(),
        frame_memory: af_stream::MemoryLimits::default(),
        wall_clock: Arc::new(af_core::clock::SystemClock),
        analytics: af_observability::Analytics::silent(),
    });
    Ok((gateway, drafts))
}

pub(in crate::lane::run) async fn open(
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
