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

/// Records an installation for the workspace that holds it, and ONLY that one.
///
/// [`UPSERT_INSTALL`]'s exclusive twin. The conflict arm updates the row only
/// when the existing row already names this workspace — a reconnect refreshes
/// it — and touches nothing otherwise, so a zero row count is the signal that
/// another workspace holds the installation. `github/sql.zig` answered the
/// same question with `RETURNING` on a guarded upsert; the count is the same
/// fact without a second round trip.
pub const CLAIM_INSTALL: &str = "\
INSERT INTO core.connector_installs \
  (id, provider, external_account_id, workspace_id, installed_by, scopes, created_at, updated_at) \
VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6::text[], $7, $7) \
ON CONFLICT (provider, external_account_id) DO UPDATE SET \
  installed_by = EXCLUDED.installed_by, \
  scopes = EXCLUDED.scopes, \
  updated_at = EXCLUDED.updated_at \
WHERE core.connector_installs.workspace_id = EXCLUDED.workspace_id";

/// Lets go of every OTHER account this workspace routes for the provider.
///
/// The vault holds ONE handle per workspace and provider — the secret is named
/// [`Provider::grant_key`], which is the bare provider id — so a workspace can
/// only ever spend one account's credential. The routing table has no such
/// bound: its unique key is `(provider, external_account_id)`, so connecting a
/// second account inserts a second row rather than replacing the first, and
/// both rows name this workspace.
///
/// What survives that is the state [`Grants::land`]'s note calls the silent
/// one: the old account's deliveries still resolve here through
/// [`SELECT_INSTALL_WORKSPACE`], and outbound answers them with a credential
/// minted for the new one. Nothing reads as broken; the runs just fail at the
/// vendor. So a connect releases the rows it is about to stop being able to
/// serve, in the same transaction that claims the new one.
///
/// [`Provider::grant_key`]: crate::provider::Provider::grant_key
/// [`Grants::land`]: crate::grant::Grants::land
pub const RELEASE_OTHER_INSTALLS: &str = "\
DELETE FROM core.connector_installs \
WHERE provider = $1 AND workspace_id = $2::uuid AND external_account_id <> $3";

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
    use super::{
        CLAIM_INSTALL, DELETE_WORKSPACE_INSTALLS, RELEASE_OTHER_INSTALLS, SELECT_INSTALL_WORKSPACE,
        UPSERT_INSTALL,
    };

    /// Every statement names its schema, so none of them depends on a
    /// `search_path` a pooled connection could have been handed.
    #[test]
    fn every_statement_is_schema_qualified() {
        for statement in [
            UPSERT_INSTALL,
            CLAIM_INSTALL,
            RELEASE_OTHER_INSTALLS,
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

    /// The exclusive claim never moves a row another workspace holds.
    ///
    /// The guard is the `WHERE` on the conflict arm: without it this statement
    /// IS the re-pointing upsert, and a second workspace connecting the same
    /// installation would silently take the first one's pull requests.
    #[test]
    fn the_exclusive_claim_updates_only_the_holding_workspace() {
        assert!(CLAIM_INSTALL.contains("ON CONFLICT (provider, external_account_id) DO UPDATE"));
        assert!(
            CLAIM_INSTALL
                .contains("WHERE core.connector_installs.workspace_id = EXCLUDED.workspace_id")
        );
        assert!(!CLAIM_INSTALL.contains("workspace_id = EXCLUDED.workspace_id,"));
    }

    /// The disconnect is scoped to one workspace as well as one provider.
    #[test]
    fn the_disconnect_cannot_reach_another_workspaces_rows() {
        assert!(DELETE_WORKSPACE_INSTALLS.contains("workspace_id = $2::uuid"));
    }

    /// The release is bounded by BOTH the workspace and the provider.
    ///
    /// Without the workspace arm it would delete another tenant's routing;
    /// without the provider arm, this workspace's Slack row would go when it
    /// connected GitHub. The `<>` is what keeps the row being claimed.
    #[test]
    fn the_release_never_reaches_past_this_workspace_and_provider() {
        assert!(RELEASE_OTHER_INSTALLS.contains("provider = $1"));
        assert!(RELEASE_OTHER_INSTALLS.contains("workspace_id = $2::uuid"));
        assert!(RELEASE_OTHER_INSTALLS.contains("external_account_id <> $3"));
    }
}
