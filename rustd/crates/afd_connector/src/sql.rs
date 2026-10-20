//! The statements this crate runs, in one place a reader can grep.
//!
//! Public so a suite can take the same lock a writer takes without respelling
//! it — the same reason `afd_cron::sql` and `afd_ingress::sql` are public. Nothing here is
//! composed at runtime: every statement is a `.sql` file under `sql/`, every
//! value a caller supplies is a bind parameter, and every stamp is the
//! database clock's `now()`.

/// Records which workspace a provider account's inbound events belong to.
///
/// The conflict target is the provider and the account rather than the
/// workspace, because an account can be MOVED: a Slack team reinstalled into a
/// different workspace must re-point, not accumulate a second row that two
/// readers would resolve differently.
pub const UPSERT_INSTALL: &str = include_str!("sql/upsert_install.sql");

/// Records an installation for the workspace that holds it, and ONLY that one.
///
/// [`UPSERT_INSTALL`]'s exclusive twin. The conflict arm updates the row only
/// when the existing row already names this workspace — a reconnect refreshes
/// it — and touches nothing otherwise, so a zero row count is the signal that
/// another workspace holds the installation. The row count answers that
/// without a `RETURNING` clause or a second round trip.
///
/// `$1` id · `$2` provider · `$3` account · `$4` workspace · `$5` scopes, the
/// binds both install statements share.
pub const CLAIM_INSTALL: &str = include_str!("sql/claim_install.sql");

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
/// one: the old account's deliveries still resolve here — the ingress crate
/// owns the routing read — and outbound answers them with a credential minted
/// for the new one. Nothing reads as broken; the runs just fail at the vendor.
/// So a connect releases the rows it is about to stop being able to serve, in
/// the same transaction that claims the new one.
///
/// [`Provider::grant_key`]: crate::provider::Provider::grant_key
/// [`Grants::land`]: crate::grant::Grants::land
pub const RELEASE_OTHER_INSTALLS: &str = include_str!("sql/release_other_installs.sql");

/// Forgets an account's routing rows when a workspace disconnects a provider.
///
/// Scoped to the workspace as well as the provider: a disconnect must not
/// remove a row another workspace owns, and the two-column predicate is what
/// makes that a property of the statement rather than of the caller.
pub const DELETE_WORKSPACE_INSTALLS: &str = include_str!("sql/delete_workspace_installs.sql");
