//! Undoing an install whose stream never came up.
//!
//! Split from [`super`] because it is the one path there that runs when
//! everything else has already failed, and it carries its own promise: when
//! `install` answers an error, the `core.fleets` row it wrote is gone and
//! retrying is safe. That promise is worth reading in one piece.

use afd_core::error_code;
use afd_core::id::Uuid7;

use crate::error::{self, ErrorKind};
use crate::{Fleets, sql};

/// The context the rollback reports a failed statement under.
const CONTEXT_ROLLBACK: &str = "roll back installed fleet";

/// The hint a rollback that itself failed leaves for an operator to grep.
const HINT_ORPHANED: &str = "row_orphaned_manual_recovery";

impl Fleets {
    /// Deletes the row an install could not finish, on a FRESH connection.
    ///
    /// Fresh because the request's was released before Redis was reached, and
    /// because a rollback queued behind the same exhausted pool would fail for
    /// the reason it is running.
    ///
    /// Answers [`ErrorKind::InstallRolledBack`] either way. A rollback that
    /// itself fails leaves an orphan row and logs a hint an operator can grep,
    /// but the CALLER is in the same position regardless: the fleet is unusable
    /// and retrying is the only move they have.
    pub(super) async fn roll_back(
        &self,
        workspace: &Uuid7,
        id: &Uuid7,
        cause: &crate::Error,
    ) -> crate::Error {
        let fleet = id.as_str();
        let reason = cause.to_string();
        let removed = async {
            let mut connection = self.database.acquire().await?;
            sqlx::query(sql::DELETE_FLEET)
                .bind(fleet)
                .bind(workspace.as_str())
                .execute(connection.as_mut())
                .await
                .map_err(error::query(CONTEXT_ROLLBACK))?;
            Ok::<(), crate::Error>(())
        }
        .await;

        match removed {
            Ok(()) => tracing::warn!(
                error_code = error_code::AGENTSFLEET_INSTALL_ROLLED_BACK.as_str(),
                fleet,
                reason,
                event = "install_rolled_back",
            ),
            Err(double_fault) => {
                let rollback_error = double_fault.to_string();
                tracing::error!(
                    error_code = error_code::AGENTSFLEET_INSTALL_ROLLED_BACK.as_str(),
                    fleet,
                    reason,
                    rollback_error,
                    hint = HINT_ORPHANED,
                    event = "install_rollback_failed",
                );
            }
        }
        ErrorKind::InstallRolledBack.into()
    }
}
