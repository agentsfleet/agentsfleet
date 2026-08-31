//! Landing a connection: the handle sealed in the vault, and the row that
//! routes the provider's events back.
//!
//! # The vault write is a claim-then-replace, and the order is deliberate
//!
//! `afd_vault` offers a create that refuses a name already held and a replace
//! that refuses one that is not — deliberately, because the workspace secret
//! surface wants both refusals. A connect wants neither: connecting for the
//! first time and RECONNECTING are the same action to whoever pressed the
//! button, and a person told "you already connected Slack" when their token was
//! revoked has been given a dead end. So the create runs first and its
//! name-taken refusal is read as "this is a reconnect", not as a failure.
//!
//! Two connects racing on one provider resolve to one grant — whichever wrote
//! last — which is the same outcome `binding_tx.zig`'s advisory lock produces,
//! reached without holding a lock across a vendor call.
//!
//! # The routing row is written AFTER the grant, never before
//!
//! A row saying "this Slack team belongs to this workspace" with no vaulted bot
//! token behind it is an ingress that resolves a workspace and then cannot
//! answer. The other order leaves a grant nothing routes to yet, which is the
//! state a reconnect is in for a millisecond anyway.

pub mod holding;
pub mod parse;

use std::sync::atomic::{AtomicI64, Ordering};

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_vault::{SecretBody, SecretName, Vault};

use crate::error::{Result, query};
use crate::provider::Provider;
use crate::sql;

pub use self::holding::{Connection, Forgotten};
pub use self::parse::{Grant, Install};

/// The context a failed install write reports under.
const CONTEXT_INSTALL: &str = "record a connector install";

/// Why the handle's serialization cannot fail — see [`Grants::land`].
///
/// Named once rather than spelled at the site, so the invariant reads as a
/// stated fact rather than as a hopeful message.
const HANDLE_IS_ALWAYS_SERIALIZABLE: &str =
    "a JSON object of strings and integers always serializes";

/// The last connect stamp this process handed out.
///
/// `oauth_refresh.zig`'s `last_connect_stamp`, and it exists for a subtle
/// reason worth restating: the broker's credential cache is keyed on a
/// fingerprint over the handle's non-rotating fields, and several of those are
/// constants (a label) or account-scoped (a data centre). Without a strictly
/// increasing stamp, reconnecting to a DIFFERENT account inside one millisecond
/// would produce the same fingerprint and keep serving the previous account's
/// cached token until it expired.
static LAST_CONNECT_STAMP: AtomicI64 = AtomicI64::new(0);

/// A connect stamp strictly later than every one this process has handed out.
///
/// Monotonic rather than merely wall-clock — see [`LAST_CONNECT_STAMP`].
fn connected_at(now: UnixMillis) -> UnixMillis {
    let mut previous = LAST_CONNECT_STAMP.load(Ordering::Relaxed);
    loop {
        let next = now.as_millis().max(previous.saturating_add(1));
        match LAST_CONNECT_STAMP.compare_exchange_weak(
            previous,
            next,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_won) => return UnixMillis::from_millis(next),
            // A stale read costs one retry; the exchange re-validates.
            Err(current) => previous = current,
        }
    }
}

/// Where a connection lands: the vault it is sealed in and the rows it routes.
///
/// Cheap to clone — [`Db`] is a handle over a shared pool and [`Vault`] holds
/// its key behind an `Arc`.
#[derive(Debug, Clone)]
pub struct Grants {
    /// Where the handle is sealed.
    vault: Vault,
    /// Where the routing row is written.
    database: Db,
    /// Where the routing row's identifier comes from.
    ///
    /// Held rather than built per call: an entropy source is a handle on the
    /// system's, and a connect is rare enough that the handle costs nothing and
    /// frequent enough that building one per call is pure waste.
    entropy: Entropy,
}

impl Grants {
    /// Binds the store to an opened vault, pool and entropy source.
    #[must_use]
    pub const fn new(vault: Vault, database: Db, entropy: Entropy) -> Self {
        Self {
            vault,
            database,
            entropy,
        }
    }

    /// A connect stamp for a grant about to be sealed — see [`connected_at`].
    #[must_use]
    pub fn stamp(&self, now: UnixMillis) -> UnixMillis {
        connected_at(now)
    }

    /// Seals `grant` for `workspace` and routes whatever it routes.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, an envelope that would not
    /// seal, and a handle past the bound a vault row may hold. A name already
    /// held is NOT an error — see the module note on why a reconnect is the
    /// same action as a connect.
    ///
    /// # Panics
    /// Cannot. The one `expect` below serializes a `Map` of strings and
    /// integers this crate built, and `serde_json` reports a failure there only
    /// for a refusing `Serialize` or a non-string key — neither constructible
    /// here. The paragraph beside it carries the argument.
    pub async fn land(
        &self,
        workspace: &Uuid7,
        provider: Provider,
        grant: &Grant,
        now: UnixMillis,
    ) -> Result<()> {
        let name = SecretName::parse(provider.grant_key())?;
        // Rendered through `RawValue` rather than handed over as a tree: the
        // vault stores the BYTES it was given, and `SecretBody` is the type
        // that says those bytes are a non-empty JSON object within bound.
        //
        // Infallible, and the reason is structural rather than statistical: the
        // handle is a `Map<String, Value>` this crate built out of strings and
        // integers, and serializing one has no error case — the failures
        // `to_raw_value` reports come from a custom `Serialize` refusing or
        // from a non-string map key, and neither is constructible here.
        #[expect(
            clippy::expect_used,
            reason = "a serde_json Map of strings and integers has no serialization failure; \
                      see the paragraph above for why the arm is unconstructible"
        )]
        let raw =
            serde_json::value::to_raw_value(&grant.handle).expect(HANDLE_IS_ALWAYS_SERIALIZABLE);
        let body = SecretBody::parse(&raw)?;

        // Routing BEFORE the vault, and the order is the whole failure story.
        // These are two writes with no transaction between them, so one of them
        // can land alone — and which one decides what a person sees.
        //
        // Vault first would leave the grant sealed and the account unrouted:
        // connector status reads CONNECTED, inbound deliveries resolve no
        // workspace, and the callback state is already spent, so there is no
        // signal to reconnect and no way to retry the half that failed.
        //
        // This way round the survivor is a routing row with no grant. Status
        // reads NOT connected — which is true, nothing can be spent — and
        // reconnecting re-runs both writes over an upsert that arbitrates on
        // `(provider, external_account_id)`. The visible state is the
        // pessimistic one, and the fix is the button the person already has.
        if let Some(install) = grant.install.as_ref() {
            self.route(workspace, provider, install, now).await?;
        }

        match self.vault.create(workspace, &name, &body, now).await {
            Ok(()) => {}
            Err(refused) if refused.code() == error_code::SECRET_NAME_TAKEN => {
                self.vault.replace(workspace, &name, &body, now).await?;
            }
            Err(other) => return Err(other.into()),
        }

        tracing::info!(
            workspace_id = workspace.as_str(),
            provider = provider.id(),
            event = "connector_connected",
        );
        Ok(())
    }

    /// Points this provider account's inbound events at `workspace`.
    async fn route(
        &self,
        workspace: &Uuid7,
        provider: Provider,
        install: &Install,
        now: UnixMillis,
    ) -> Result<()> {
        let mut bytes = [0_u8; ENTROPY_LEN];
        self.entropy.fill(&mut bytes)?;
        let id = Uuid7::encode(now, bytes)?;

        let mut connection = self.database.acquire().await?;
        sqlx::query(sql::UPSERT_INSTALL)
            .bind(id.as_str())
            .bind(provider.id())
            .bind(&install.external_account_id)
            .bind(workspace.as_str())
            .bind(&install.installed_by)
            .bind(&install.scopes)
            .bind(now.as_millis())
            .execute(connection.as_mut())
            .await
            .map_err(query(CONTEXT_INSTALL))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use afd_core::clock::UnixMillis;

    use super::connected_at;

    /// Two stamps from one instant are still strictly increasing.
    ///
    /// The property the broker's cache fingerprint depends on: without it, a
    /// reconnect to a different account inside one millisecond keeps serving
    /// the previous account's cached token — see [`super::LAST_CONNECT_STAMP`].
    #[test]
    fn two_connects_in_one_millisecond_do_not_share_a_stamp() {
        let instant = UnixMillis::from_millis(1_700_000_000_000);

        let first = connected_at(instant);
        let second = connected_at(instant);

        assert!(
            second.as_millis() > first.as_millis(),
            "{} must follow {}",
            second.as_millis(),
            first.as_millis(),
        );
    }

    /// A later wall clock wins over the counter, rather than trailing it.
    ///
    /// The stamp is a real instant that happens to be monotonic, not a
    /// sequence: a handle whose `connected_at_ms` drifted away from the clock
    /// would misreport when a workspace connected.
    #[test]
    fn a_later_instant_is_taken_rather_than_the_previous_stamp_plus_one() {
        let early = connected_at(UnixMillis::from_millis(1_700_000_000_000));
        let far_later = 1_800_000_000_000;

        let late = connected_at(UnixMillis::from_millis(far_later));

        assert!(late.as_millis() > early.as_millis());
        assert_eq!(late.as_millis(), far_later);
    }
}
