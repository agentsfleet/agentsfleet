//! What a fake sandbox does once built: serves, parks, resumes, dies and is
//! destroyed, as its engine's knobs say.

use std::sync::atomic::Ordering;

use af_core::id::Uuid7;
use afr_engine::{
    Environment, Granted, Killed, Network, ParkMode, Parked, Quiescence, Sandbox, SandboxState,
};
use afr_executor::Executor;

use super::{FakeSandbox, KILL, Kill, NO_PARK, NO_QUIESCENCE, NO_REFILL, Parking, STILL_RUNNING};

#[async_trait::async_trait]
impl Sandbox for FakeSandbox {
    fn id(&self) -> &Uuid7 {
        &self.id
    }

    fn environment(&self) -> &Environment {
        &self.environment
    }

    fn granted(&self) -> &Granted {
        &self.granted
    }

    fn executor(&self) -> &dyn Executor {
        &self.executor
    }

    fn state(&self) -> SandboxState {
        match self.parking {
            Parking::Lost => SandboxState::Ended,
            Parking::Dead if self.executor.served.load(Ordering::SeqCst) => SandboxState::Ended,
            _running => SandboxState::Running,
        }
    }

    async fn quiesce(&self) -> afr_engine::Result<Quiescence> {
        self.quiesced.fetch_add(1, Ordering::SeqCst);
        if !self.quiesce_takes.is_zero() {
            tokio::time::sleep(self.quiesce_takes).await;
        }
        if self.parking == Parking::QuiescenceUnproved {
            return Err(std::io::Error::other(NO_QUIESCENCE).into());
        }
        if self.parking == Parking::BusyTenant {
            return Ok(Quiescence::Busy);
        }
        Ok(Quiescence::Quiet)
    }

    async fn park(self: Box<Self>, mode: ParkMode) -> afr_engine::Result<Box<dyn Parked>> {
        if self.parking == Parking::RefusesPark {
            return Err(self.destroyed_on(NO_PARK).await);
        }
        self.parked.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeParked {
            sandbox: self,
            mode,
        }))
    }

    /// Records an allowlist plan and refuses one as asked; a plan for the
    /// network the sandbox already reaches changes nothing.
    async fn egress(&mut self, plan: afr_engine::EgressPlan<'_>) -> afr_engine::Result<()> {
        let Network::Allowed(allowlist) = plan.network else {
            return Ok(());
        };
        if self.parking == Parking::RefusesReallow {
            return Err(std::io::Error::other(NO_REFILL).into());
        }
        if let Some(reallowed) = &self.reallowed {
            let resumed = self.executor.resumed.load(Ordering::SeqCst);
            // A test that dropped its receiver is not watching reallows.
            drop(reallowed.send((allowlist.clone(), resumed)));
        }
        Ok(())
    }

    async fn kill(&self) -> Killed {
        self.step(KILL);
        if self.kill == Kill::Unsettles {
            return Killed::Unsettled(std::io::Error::other(STILL_RUNNING).into());
        }
        Killed::Settled
    }

    fn snapshots(&self) -> Option<&dyn afr_engine::Snapshots> {
        None
    }

    async fn destroy(self: Box<Self>) -> afr_engine::Result<()> {
        self.teardown().await
    }
}

/// A fake sandbox, parked.
#[derive(Debug)]
struct FakeParked {
    sandbox: Box<FakeSandbox>,
    mode: ParkMode,
}

#[async_trait::async_trait]
impl Parked for FakeParked {
    fn id(&self) -> &Uuid7 {
        &self.sandbox.id
    }

    fn mode(&self) -> ParkMode {
        self.mode
    }

    async fn resume(self: Box<Self>) -> afr_engine::Result<Box<dyn Sandbox>> {
        let sandbox = self.sandbox;
        sandbox.resume_attempts.fetch_add(1, Ordering::SeqCst);
        if !sandbox.resume_takes.is_zero() {
            tokio::time::sleep(sandbox.resume_takes).await;
        }
        if sandbox.parking == Parking::RefusesResume {
            return Err(sandbox.destroyed_on(NO_PARK).await);
        }
        sandbox.resumed.fetch_add(1, Ordering::SeqCst);
        sandbox.executor.resumed.store(true, Ordering::SeqCst);
        Ok(sandbox)
    }

    async fn destroy(self: Box<Self>) -> afr_engine::Result<()> {
        self.sandbox.teardown().await
    }
}
