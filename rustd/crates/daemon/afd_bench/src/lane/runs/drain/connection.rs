//! Compose the same gateway handles as daemon boot, before starting measured windows.

use super::stage;
use crate::lane::runs::seed::RunnerIdentity;
use crate::{datastores::Datastores, error::Result};
use af_crypto::{entropy::Entropy, secret::Kek};
use afd_gateway::{Gateway, GatewayDependencies, test_util::RunnerConnection};
use std::sync::Arc;

pub(in crate::lane::runs) fn gateway(stores: &Datastores, kek: &Arc<Kek>) -> Gateway {
    let runs = stage::plane(stores, kek);
    Gateway::new(GatewayDependencies {
        recovery: afd_gateway::RecoverySettings::default(),
        database: stores.database.clone(),
        runners: afd_runner::Runners::new(stores.database.clone(), Entropy::new()),
        runs,
        bus: stores.bus.clone(),
        stream_limit: afd_sse::StreamLimit::new(usize::MAX),
        wake: afd_bus::wake::Wake::default(),
        frame_memory: af_stream::MemoryLimits::default(),
        wall_clock: Arc::new(af_core::clock::SystemClock),
        analytics: af_observability::Analytics::silent(),
        frames: afd_gateway::FrameCeilings::default(),
    })
}

pub(in crate::lane::runs) async fn open(
    gateway: &Gateway,
    runner: &RunnerIdentity,
    credits: usize,
) -> Result<RunnerConnection> {
    Ok(RunnerConnection::open(gateway, runner.id.clone(), runner.digest.clone(), credits).await?)
}
