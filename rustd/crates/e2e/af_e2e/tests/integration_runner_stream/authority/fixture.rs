//! Two real daemons share the lane's Postgres and NATS, with one fixture-owned operator.

use super::{TestResult, operator::Operator};
use crate::e2e::{self, Scenario};
use af_crypto::entropy::Entropy;
use af_stream::Transport;
use afd_bus::Handle;
use afd_setup::Creations;
use afd_turns::Turns;
use agentsfleetd::{
    serve::{Booted, boot},
    supervisor::Supervisor,
};

use super::super::raw::Native;

pub(super) struct Fixture {
    pub(super) scene: Scenario,
    pub(super) peer: Booted,
    pub(super) bus: Handle,
    pub(super) operator: Operator,
    primary: Supervisor,
    replica: Supervisor,
    siblings: Vec<af_core::id::Uuid7>,
}

impl Fixture {
    pub(super) async fn start() -> TestResult<Self> {
        let operator = Operator::start().await?;
        let mut primary = Supervisor::new();
        let scene = e2e::scenario_with_provider(&mut primary, Some(&operator.provider)).await;
        operator.seed(&scene).await?;
        let mut replica = Supervisor::new();
        let environment = e2e::daemon_environment(
            &e2e::lane(e2e::DATABASE_LANE_KNOB),
            Some(&operator.provider),
            &[],
        );
        let peer = boot(&environment, e2e::EPHEMERAL, &mut replica).await?;
        let bus = scene.booted.bus.clone();
        Ok(Self {
            scene,
            peer,
            bus,
            operator,
            primary,
            replica,
            siblings: Vec::new(),
        })
    }

    pub(super) async fn native(&self, transport: Transport) -> Native {
        Native::connect_at(
            self.peer.runner_stream_address,
            &self.scene.token,
            transport,
        )
        .await
    }

    /// The fleet owner on `bus`, as the tenant plane builds it.
    pub(super) fn creations(&self, bus: Handle) -> Creations {
        let database = self.scene.booted.database.clone();
        let turns = Turns::new(database.clone(), bus, Entropy::new());
        Creations::new(database, Entropy::new(), turns)
    }

    /// Retires `fleet` with the scenario's, for a fleet the test made itself.
    pub(super) fn adopt(&mut self, fleet: af_core::id::Uuid7) {
        self.siblings.push(fleet);
    }

    pub(super) async fn sibling(&mut self, name: &str) -> (af_core::id::Uuid7, af_core::id::Uuid7) {
        let sibling = super::super::population::sibling(&self.scene, name).await;
        self.siblings.push(sibling.0.clone());
        sibling
    }

    pub(super) async fn finish(self) -> TestResult {
        self.replica.shutdown().await;
        self.primary.shutdown().await;
        for sibling in &self.siblings {
            crate::e2e_retire::retire_fleet(&self.scene.booted, sibling).await;
        }
        self.operator.finish(&self.scene).await?;
        drop(self.peer);
        self.scene.cleanup().await;
        Ok(())
    }
}
