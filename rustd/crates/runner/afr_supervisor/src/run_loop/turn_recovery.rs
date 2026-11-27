//! The same fresh sandbox supplies the file destination and the restored root witness;
//! its files land through its engine before anything is adopted.

use std::panic::AssertUnwindSafe;

use afr_engine::Sandbox;
use futures_util::FutureExt as _;

use super::recovery::prove_quiescent;
use super::{Run, build::Build, recovery::State};
use crate::{
    Result, error,
    recovery::{Landing, NativeTurn, Taken},
};

impl Run<'_> {
    pub(super) async fn prepare_turn_recovery(
        &self,
        sandbox: &dyn Sandbox,
        build: &Build<'_>,
    ) -> Result<()> {
        let actual = Taken::turn_workspace(sandbox.environment())
            .ok_or_else(|| error::config("ordinary restore has no verified toolbox"))?;
        // Held until the files land: it bounds the restores the host runs
        // at once.
        let _permit = self.lessee.recovery.file(build.stop).await?;
        let landing = Landing {
            sandbox,
            spools: self.lessee.staging.root(),
        };
        let restoring = async {
            prove_quiescent(sandbox).await?;
            NativeTurn::new(
                &self.lessee.plane,
                &self.ids.run,
                self.payload.fencing_token,
                &self.lessee.recovery.transfers,
            )
            .restore(self.payload, actual, Some(landing), &self.interrupt)
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
