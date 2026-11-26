//! Only the controlled model is reachable. A file/tool workspace is an invalid fixture.

use std::{io, net::SocketAddr, path::Path};

use af_core::env::ProcessEnv;
use af_wire::runner::{Isolation, SandboxEngine};
use afr_engine::{
    Cap, Capabilities, Engine, Granted, Limits, ResourceFacts, Sandbox, SandboxSpec, Sweep, Swept,
};
use tokio_util::sync::CancellationToken;

use super::Checked;

#[derive(Debug)]
struct ApiOnly;

#[async_trait::async_trait]
impl Engine for ApiOnly {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            engine: SandboxEngine::Unsandboxed,
            isolation: Isolation::Unconfined,
            parks: &[],
            snapshots: false,
            resources: ResourceFacts {
                memory: Cap::Absent,
                cpu: Cap::Absent,
                pids: Cap::Absent,
                disk: Cap::Absent,
            },
        }
    }

    fn grant(&self, _limits: Limits) -> afr_engine::Result<Granted> {
        Err(io::Error::other("API-only benchmark received a sandbox operation").into())
    }

    fn memory_bound(&self) -> Option<afr_engine::MemoryBound> {
        None
    }

    async fn prepare(&self, _spec: SandboxSpec<'_>) -> afr_engine::Result<Box<dyn Sandbox>> {
        Err(io::Error::other("API-only benchmark received a sandbox operation").into())
    }

    async fn sweep(&self, _scope: Sweep<'_>) -> afr_engine::Result<Swept> {
        Ok(Swept::default())
    }
}

pub(super) async fn run(address: SocketAddr, root: &Path) -> Checked {
    let (config, home) = afr_supervisor::boot(&ProcessEnv)?;
    let client = guffbolt_ai::testing::tls::client_from_file(root, address)?;
    let agent = afr_agent::Loop::new(
        afr_tools::Catalog::new(Vec::new()),
        af_providers::Connector::with_client(client),
    );
    let probe = afr_engine_bubblewrap::probe(&afr_engine_bubblewrap::ProbePaths::default());
    let stop = CancellationToken::new();
    let signal = stop.clone();
    let handler = tokio::spawn(async move {
        af_core::signal::shutdown().await;
        signal.cancel();
    });
    let result = afr_supervisor::run(
        &config,
        home,
        Box::new(ApiOnly),
        Box::new(agent),
        probe,
        stop,
    )
    .await;
    handler.abort();
    let _joined = handler.await;
    Ok(result?)
}
