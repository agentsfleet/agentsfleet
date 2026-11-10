//! Two real daemons share the lane's Postgres and NATS, with one fixture-owned operator.

use super::{TestResult, operator::Operator};
use crate::e2e::{self, Scenario};
use af_core::clock;
use af_crypto::{entropy::Entropy, secret::Kek};
use af_stream::Transport;
use afd_admission::Admissions;
use afd_authoring::{Creations, Drafts, Setup, SystemSetup};
use afd_bus::Handle;
use agentsfleetd::{
    serve::{Booted, boot},
    supervisor::Supervisor,
};
use std::sync::Arc;

use super::super::raw::Native;

pub(super) struct Fixture {
    pub(super) run: Scenario,
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
        let run = e2e::scenario_with_provider(&mut primary, Some(&operator.provider)).await;
        operator.seed(&run).await?;
        let mut replica = Supervisor::new();
        let environment = e2e::daemon_environment(
            &e2e::lane(e2e::DATABASE_LANE_KNOB),
            Some(&operator.provider),
            &[],
        );
        let peer = boot(&environment, e2e::EPHEMERAL, &mut replica).await?;
        let bus = run.booted.bus.clone();
        let mut fixture = Self {
            run,
            peer,
            bus,
            operator,
            primary,
            replica,
            siblings: Vec::new(),
        };
        fixture.reviewed_fleet().await?;
        Ok(fixture)
    }

    /// Kill reads a saved review, so install through the production setup owner.
    async fn reviewed_fleet(&mut self) -> TestResult {
        let mut config: serde_json::Value =
            serde_json::from_str(crate::e2e_seed::FLEET_CONFIG_JSON)?;
        config
            .as_object_mut()
            .ok_or("the seed fleet configuration is an object")?
            .insert("name".to_owned(), "cancellation-review".into());
        let definition = crate::e2e_native::definition(
            &config.to_string(),
            crate::e2e_seed::FIXTURE_INSTRUCTIONS,
        );
        let fleet = self
            .creations(self.bus.clone())?
            .materialise(
                SystemSetup {
                    workspace: &self.run.workspace,
                    definition: &definition,
                },
                clock::now(),
            )
            .await?;
        crate::e2e_retire::retire_fleet(&self.run.booted, &self.run.fleet).await;
        self.run.fleet = fleet;
        let registered = afd_runner::Runners::new(self.run.booted.database.clone(), Entropy::new())
            .register(
                &crate::e2e_seed::scenario_enrolment(&self.run.fleet),
                clock::now(),
            )
            .await?;
        self.run.runner_id = registered.runner_id;
        registered.token.expose().clone_into(&mut self.run.token);
        self.run.event_id = self.run.enqueue_event(e2e::EVENT_TYPE).await;
        Ok(())
    }

    pub(super) async fn native(&self, transport: Transport) -> Native {
        Native::connect_at(self.peer.runner_stream_address, &self.run.token, transport).await
    }

    pub(super) fn creations(&self, bus: Handle) -> TestResult<Creations> {
        let database = self.run.booted.database.clone();
        let drafts = Drafts::new(
            database.clone(),
            Entropy::new(),
            Arc::new(afd_billing::ledger::LedgerBiller::on_system_clock(
                database.clone(),
            )),
        );
        let setup = Setup::new(drafts, Arc::new(Kek::from_hex(e2e::GOOD_KEK)?));
        Ok(Creations::new(
            setup,
            Admissions::new(database, bus, Entropy::new()),
        ))
    }

    pub(super) async fn sibling(&mut self, name: &str) -> (af_core::id::Uuid7, af_core::id::Uuid7) {
        let sibling = super::super::population::sibling(&self.run, name).await;
        self.siblings.push(sibling.0.clone());
        sibling
    }

    pub(super) async fn finish(self) -> TestResult {
        self.replica.shutdown().await;
        self.primary.shutdown().await;
        for sibling in &self.siblings {
            crate::e2e_retire::retire_fleet(&self.run.booted, sibling).await;
        }
        self.operator.finish(&self.run).await?;
        drop(self.peer);
        self.run.cleanup().await;
        Ok(())
    }
}
