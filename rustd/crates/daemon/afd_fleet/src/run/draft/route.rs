//! The key a draft turn calls its model with, opened inside the assignment's
//! transaction.

use af_wire::setup::FailureReason;
use afd_credential::provider::RegisteredRoute;
use afd_setup::setup_run::SetupTurn;

use crate::error::Result;
use crate::run::RunService;

impl RunService {
    /// The key `turn` calls its model with, opened in the assignment's
    /// transaction: `None` when the stored selection moved since the model
    /// was pinned, and when the stored key or selection is wrong, which is
    /// logged. Either way the caller fails this turn alone, so the walk goes
    /// on, and the runner's fleet work after it.
    pub(super) async fn route(
        &self,
        tx: &mut sqlx::PgConnection,
        turn: &SetupTurn,
    ) -> Result<Option<RegisteredRoute>> {
        match self
            .providers
            .registered_route(tx, &turn.tenant_id, &turn.model)
            .await
        {
            Ok(route) => Ok(route),
            Err(fault) if !fault.is_retryable() => {
                afd_setup::setup_run::log_turn_failed(
                    &turn.draft_id,
                    FailureReason::ModelUnavailable,
                    fault.code(),
                );
                Ok(None)
            }
            Err(fault) => Err(fault.into()),
        }
    }
}
