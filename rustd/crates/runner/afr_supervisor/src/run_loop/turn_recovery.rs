//! The same fresh sandbox supplies the file destination and the restored root witness.

use super::{Run, build::Build, recovery::State};
use crate::{Result, error, recovery::NativeTurn};
use af_wire::run::TurnWorkspace;
use afr_sandbox::Sandbox;
use futures_util::FutureExt as _;
use std::panic::AssertUnwindSafe;

use super::recovery::prove_quiescent;

impl Run<'_> {
    pub(super) async fn prepare_turn_recovery(
        &self,
        sandbox: &dyn Sandbox,
        build: &Build<'_>,
    ) -> Result<()> {
        let workspace = sandbox
            .workspace()
            .ok_or_else(|| error::config("ordinary restore has no trusted workspace"))?;
        let toolbox = sandbox
            .environment()
            .root
            .toolbox()
            .ok_or_else(|| error::config("ordinary restore has no verified toolbox"))?;
        let actual = TurnWorkspace::Files {
            toolbox_digest: toolbox.to_owned(),
        };
        let permit = self.lessee.recovery.file(build.stop).await?;
        let restoring = async {
            prove_quiescent(sandbox).await?;
            NativeTurn::new(
                &self.lessee.plane,
                &self.ids.run,
                self.payload.fencing_token,
                &self.lessee.recovery.transfers,
            )
            .restore(
                self.payload,
                actual,
                Some((workspace.root.to_owned(), workspace.owner)),
                &self.interrupt,
                Some(&permit),
            )
            .await
        };
        let turn = AssertUnwindSafe(restoring)
            .catch_unwind()
            .await
            .map_err(|_panic| error::config("ordinary restore panicked"))??;
        if turn.is_none() {
            return Err(error::config(
                "ordinary restore lost its assigned checkpoint",
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
