//! Compose the same gateway handles as daemon boot, before starting measured windows.

use super::stage;
use crate::lane::lease::seed::RunnerIdentity;
use crate::{datastores::Datastores, error::Result};
use af_crypto::{entropy::Entropy, secret::Kek};
use af_wire::lease::AcceptedKinds;
use afd_authoring::{Creations, Drafts, ModelCalls, Network, Setup};
use afd_gateway::{Gateway, GatewayDependencies, test_util::Session};
use std::sync::Arc;

pub(in crate::lane::lease) fn gateway(
    stores: &Datastores,
    kek: &Arc<Kek>,
) -> Result<(Gateway, Drafts)> {
    let leases = stage::plane(stores, kek);
    let drafts = Drafts::new(
        stores.database.clone(),
        Entropy::new(),
        Arc::clone(&leases.biller),
    );
    let models = ModelCalls::new(
        drafts.clone(),
        leases.providers.clone(),
        Arc::new(Network::new()?),
    );
    let admissions =
        afd_admission::Admissions::new(stores.database.clone(), stores.bus.clone(), Entropy::new());
    let creations = Creations::new(Setup::new(drafts.clone(), Arc::clone(kek)), admissions);
    let gateway = Gateway::new(GatewayDependencies {
        database: stores.database.clone(),
        runners: afd_runner::Runners::new(stores.database.clone(), Entropy::new()),
        leases,
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

pub(in crate::lane::lease) async fn open(
    gateway: &Gateway,
    runner: &RunnerIdentity,
    kinds: AcceptedKinds,
    credits: usize,
) -> Result<Session> {
    Ok(Session::open(
        gateway,
        runner.id.clone(),
        runner.digest.clone(),
        kinds,
        credits,
    )
    .await?)
}
