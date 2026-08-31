//! The statements this crate runs, in one place a reader can grep.
//!
//! Public so a suite can assert on the text without respelling it — the same
//! reason `afd_cron::sql` and `afd_ingress::sql` are public. Nothing here is
//! composed at runtime: every statement is a constant, and every value a caller
//! supplies is a bind parameter.

/// Records which workspace a provider account's inbound events belong to.
///
/// `slack/sql.zig`'s `UPSERT_INSTALL`, kept statement-for-statement. The
/// conflict target is the provider and the account rather than the workspace,
/// because an account can be MOVED: a Slack team reinstalled into a different
/// workspace must re-point, not accumulate a second row that two readers would
/// resolve differently.
pub const UPSERT_INSTALL: &str = "\
INSERT INTO core.connector_installs \
  (id, provider, external_account_id, workspace_id, installed_by, scopes, created_at, updated_at) \
VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6::text[], $7, $7) \
ON CONFLICT (provider, external_account_id) DO UPDATE SET \
  workspace_id = EXCLUDED.workspace_id, \
  installed_by = EXCLUDED.installed_by, \
  scopes = EXCLUDED.scopes, \
  updated_at = EXCLUDED.updated_at";

/// Which workspace a provider account's inbound events belong to.
///
/// The read half of [`UPSERT_INSTALL`], and the first statement the Slack
/// events ingress runs once a delivery has proven itself.
pub const SELECT_INSTALL_WORKSPACE: &str = "\
SELECT workspace_id::text FROM core.connector_installs \
WHERE provider = $1 AND external_account_id = $2";

/// Forgets an account's routing rows when a workspace disconnects a provider.
///
/// Scoped to the workspace as well as the provider: a disconnect must not
/// remove a row another workspace owns, and the two-column predicate is what
/// makes that a property of the statement rather than of the caller.
pub const DELETE_WORKSPACE_INSTALLS: &str = "\
DELETE FROM core.connector_installs \
WHERE provider = $1 AND workspace_id = $2::uuid";

#[cfg(test)]
mod tests {
    use super::{DELETE_WORKSPACE_INSTALLS, SELECT_INSTALL_WORKSPACE, UPSERT_INSTALL};

    /// Every statement names its schema, so none of them depends on a
    /// `search_path` a pooled connection could have been handed.
    #[test]
    fn every_statement_is_schema_qualified() {
        for statement in [
            UPSERT_INSTALL,
            SELECT_INSTALL_WORKSPACE,
            DELETE_WORKSPACE_INSTALLS,
        ] {
            assert!(
                statement.contains("core.connector_installs"),
                "`{statement}` must name its schema",
            );
        }
    }

    /// The install upsert re-points an account rather than duplicating it.
    ///
    /// Pinned because the alternative — `DO NOTHING` — is the one-character
    /// edit that turns a reinstall into a silently stale route: events would
    /// keep arriving at the workspace that installed the app FIRST.
    #[test]
    fn the_install_upsert_repoints_an_account_that_moved() {
        assert!(UPSERT_INSTALL.contains("ON CONFLICT (provider, external_account_id) DO UPDATE"));
        assert!(UPSERT_INSTALL.contains("workspace_id = EXCLUDED.workspace_id"));
    }

    /// The disconnect is scoped to one workspace as well as one provider.
    #[test]
    fn the_disconnect_cannot_reach_another_workspaces_rows() {
        assert!(DELETE_WORKSPACE_INSTALLS.contains("workspace_id = $2::uuid"));
    }
}
