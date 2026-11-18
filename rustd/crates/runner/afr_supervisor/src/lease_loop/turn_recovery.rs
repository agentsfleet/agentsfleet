//! The same fresh sandbox supplies the file destination and the restored root witness.

use super::{LeaseRun, build::Build, recovery::State};
use crate::{Result, error, recovery::NativeTurn};
use af_wire::lease::TurnWorkspace;
use afr_sandbox::Sandbox;
use futures_util::FutureExt as _;
use std::panic::AssertUnwindSafe;

impl LeaseRun<'_> {
    pub(super) async fn prepare_turn_recovery(
        &self,
        sandbox: &dyn Sandbox,
        build: &Build<'_>,
    ) -> Result<()> {
        let workspace = sandbox
            .workspace()
            .ok_or_else(|| error::config("ordinary restore has no trusted workspace"))?;
        let toolbox = sandbox
            .toolbox_digest()
            .ok_or_else(|| error::config("ordinary restore has no verified toolbox"))?;
        let actual = TurnWorkspace::Files {
            toolbox_digest: toolbox.to_owned(),
        };
        let permit = self.lessee.recovery.file(build.stop).await?;
        let restoring = async {
            sandbox.freeze().await.map_err(|source| {
                error::boundary("ordinary restore could not freeze its sandbox", source)
            })?;
            if !sandbox.recovery_quiescent().await.map_err(|source| {
                error::boundary("ordinary restore could not prove tenant quiescence", source)
            })? {
                return Err(error::config("ordinary restore has a live tenant process"));
            }
            NativeTurn::new(
                &self.lessee.plane,
                &self.ids.lease,
                self.lease.fencing_token,
                &self.lessee.recovery.transfers,
            )
            .restore(
                self.lease,
                actual,
                Some((workspace.root.to_owned(), workspace.owner)),
                &self.interrupt,
                Some(&permit),
            )
            .await
        };
        let restored = AssertUnwindSafe(restoring)
            .catch_unwind()
            .await
            .map_err(|_panic| error::config("ordinary restore panicked"));
        let thawed = AssertUnwindSafe(sandbox.thaw())
            .catch_unwind()
            .await
            .map_err(|_panic| error::config("ordinary restore thaw panicked"))?
            .map_err(|source| {
                error::boundary("ordinary restore could not thaw its sandbox", source)
            });
        let turn = restored??;
        thawed?;
        if turn.is_none() {
            return Err(error::config(
                "ordinary restore lost its offered checkpoint",
            ));
        }
        build.recovery.get_or_init(|| State {
            history: None,
            turn,
            base: None,
            base_paths: Vec::new(),
        });
        Ok(())
    }
}
