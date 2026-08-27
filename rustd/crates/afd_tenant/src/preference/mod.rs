//! Per-user, per-workspace dashboard preferences, and the onboarding checklist
//! derived beside them.
//!
//! # The server never interprets a preference
//!
//! A stored value is a client-owned JSON blob. The whole validation surface is
//! the key allowlist ([`PrefKey`]) and the byte cap ([`MAX_PREF_VALUE_BYTES`]),
//! so a new dashboard toggle costs one variant here and one in the TypeScript
//! mirror (`ui/packages/app/lib/api/preferences.ts`) — the variant's wire
//! spelling IS the key, so there is no second spelling to drift from (RULE UFS).
//!
//! # An unset bag is empty, never absent
//!
//! [`Preferences::bag`] answers an empty map for a user who has set nothing,
//! and the read never 404s. The dashboard fails open TOWARD showing onboarding,
//! so "I could not read your preferences" must look exactly like "you have set
//! none" — anything else hides the checklist from the person who needs it.
//!
//! # Why onboarding lives in this module and not its own
//!
//! The checklist is five derivable signals plus three preference keys, and the
//! consolidation is the point: the dashboard used to fire six requests for it.
//! Splitting the halves across two modules would put one HTTP call's data in
//! two places whose only relationship is that they are always read together.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use sqlx::Row as _;

use crate::sql::preference as sql;
use crate::{Result, error};

/// The cap one preference value may hold.
///
/// An opaque blob with no ceiling is free tenant storage. One kibibyte is the
/// Zig bound and the TypeScript client's, mirrored verbatim.
pub const MAX_PREF_VALUE_BYTES: usize = 1024;

/// The actor prefix a steer event carries.
///
/// `steer:` is what the messages handler stamps; the `%` makes it a prefix
/// match the index can serve. Bound as a parameter, never inlined (RULE NSQ).
const STEER_ACTOR_LIKE: &str = "steer:%";

/// A preference is truthy only when its stored text is exactly `true`.
///
/// Values round-trip byte for byte, so an exact match after trimming is the
/// whole test — no JSON parse, and no coercion of `"true"` or `1`.
const JSON_TRUE: &str = "true";

const CONTEXT_USER: &str = "preference.resolve_user";
const CONTEXT_BAG: &str = "preference.bag";
const CONTEXT_UPSERT: &str = "preference.upsert";
const CONTEXT_SIGNALS: &str = "preference.signals";
const CONTEXT_PLATFORM_MODEL: &str = "preference.platform_default_model";

/// The closed registry of writable preference keys.
///
/// A closed enum rather than an open string: the column is client-owned
/// storage, and an unbounded key space is an unbounded number of rows per user.
/// The wire spelling is the variant name, lowercased with underscores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefKey {
    /// The getting-started panel was dismissed outright.
    GettingStartedDismissed,
    /// The panel is collapsed but not dismissed.
    GettingStartedCollapsed,
    /// The person ticked the install-the-CLI step by hand.
    GettingStartedCliTicked,
}

/// The wire spelling of each registry key, bound once.
///
/// Named rather than written at both the parse and the emit site: the two
/// directions have to agree exactly, and a literal repeated across them is the
/// drift RULE UFS exists to catch — a typo in one would make a key writable
/// under a name it never lists under.
const WIRE_DISMISSED: &str = "getting_started_dismissed";
const WIRE_COLLAPSED: &str = "getting_started_collapsed";
const WIRE_CLI_TICKED: &str = "getting_started_cli_ticked";

impl PrefKey {
    /// The key this wire string names, or `None` outside the registry.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            WIRE_DISMISSED => Some(Self::GettingStartedDismissed),
            WIRE_COLLAPSED => Some(Self::GettingStartedCollapsed),
            WIRE_CLI_TICKED => Some(Self::GettingStartedCliTicked),
            _unknown => None,
        }
    }

    /// How this key is spelled on the wire and in the column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GettingStartedDismissed => WIRE_DISMISSED,
            Self::GettingStartedCollapsed => WIRE_COLLAPSED,
            Self::GettingStartedCliTicked => WIRE_CLI_TICKED,
        }
    }
}

/// One stored preference, as the column holds it.
#[derive(Debug, Clone)]
pub struct Pref {
    /// The registry key, as stored.
    pub key: String,
    /// Raw JSON text, exactly as the client wrote it.
    pub value: String,
}

/// The five signals a workspace's onboarding checklist is derived from.
///
/// Five independent booleans, and `struct_excessive_bools` is right that this
/// usually means a missing type. Not here: the checklist's wire shape is fixed
/// by parity with `OnboardingView`, each flag answers a different table, and no
/// two of them combine into a state worth naming — a workspace can hold a fleet
/// with no secret, or a steer event with no model. Collapsing them into an enum
/// would invent states the schema cannot produce.
#[expect(
    clippy::struct_excessive_bools,
    reason = "the checklist's wire shape is the parity oracle; see above"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signals {
    /// The workspace holds at least one fleet.
    pub has_fleet: bool,
    /// The workspace holds at least one vault secret.
    pub has_secret: bool,
    /// At least one event has reached the workspace.
    pub has_processed_event: bool,
    /// At least one of those events came from a steer.
    pub has_steer_event: bool,
    /// A model is resolvable — the tenant's own, or the platform default.
    pub model_configured: bool,
}

/// Reads and writes one person's preferences, and the checklist beside them.
///
/// Holds the api-role pool: every one of these reads is on a dashboard request
/// path, and a request-path read sharing a pool with background work waits
/// behind it. The entropy source draws the row identifier an upsert inserts.
#[derive(Debug, Clone)]
pub struct Preferences {
    database: Db,
    entropy: Entropy,
}

impl Preferences {
    /// A preference store over `database`, drawing identifiers from `entropy`.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// The internal user id this external subject maps to.
    ///
    /// `Ok(None)` for a subject authenticated against no `core.users` row.
    /// Inventing one here would fork identity ownership away from the signup
    /// bootstrap that owns it, so the caller refuses instead.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn resolve_user(&self, subject: &str) -> Result<Option<String>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::SELECT_USER_ID_BY_SUBJECT)
            .bind(subject)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_USER))?;

        row.map(|row| {
            row.try_get::<String, _>(0)
                .map_err(error::query(CONTEXT_USER))
        })
        .transpose()
    }

    /// Every preference this user has set in this workspace.
    ///
    /// An empty vector for a user who has set none — see the module note on why
    /// that is not an error.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn bag(&self, user: &str, workspace: &Uuid7) -> Result<Vec<Pref>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_BAG)
            .bind(user)
            .bind(workspace.as_str())
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_BAG))?;

        rows.iter()
            .map(|row| {
                let unreadable = error::query(CONTEXT_BAG);
                Ok(Pref {
                    key: row.try_get(0).map_err(&unreadable)?,
                    value: row.try_get(1).map_err(&unreadable)?,
                })
            })
            .collect()
    }

    /// Writes one key. Last-write-wins.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, or entropy that would not
    /// draw.
    pub async fn upsert(
        &self,
        user: &str,
        workspace: &Uuid7,
        key: PrefKey,
        value: &str,
        now: UnixMillis,
    ) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        sqlx::query(sql::UPSERT_PREF)
            .bind(self.mint_id(now)?.as_str())
            .bind(user)
            .bind(workspace.as_str())
            .bind(key.as_str())
            .bind(value)
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_UPSERT))?;
        Ok(())
    }

    /// Every derivable onboarding signal for one workspace.
    ///
    /// `tenant` scopes the model check; the other four are workspace-scoped.
    /// `model_configured` is true when the tenant holds its own non-empty
    /// selection OR an active platform default does — false only when no model
    /// exists anywhere, which is the only state the checklist should nag about.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn signals(&self, workspace: &Uuid7, tenant: &Uuid7) -> Result<Signals> {
        let mut connection = self.database.acquire().await?;
        let unreadable = error::query(CONTEXT_SIGNALS);
        let row = sqlx::query(sql::SELECT_SIGNALS)
            .bind(workspace.as_str())
            .bind(STEER_ACTOR_LIKE)
            .bind(tenant.as_str())
            .fetch_one(&mut *connection)
            .await
            .map_err(&unreadable)?;

        let tenant_model: bool = row.try_get(4).map_err(&unreadable)?;
        let model_configured = if tenant_model {
            true
        } else {
            sqlx::query(sql::SELECT_PLATFORM_DEFAULT_MODEL)
                .fetch_one(&mut *connection)
                .await
                .map_err(error::query(CONTEXT_PLATFORM_MODEL))?
                .try_get(0)
                .map_err(error::query(CONTEXT_PLATFORM_MODEL))?
        };

        Ok(Signals {
            has_fleet: row.try_get(0).map_err(&unreadable)?,
            has_secret: row.try_get(1).map_err(&unreadable)?,
            has_processed_event: row.try_get(2).map_err(&unreadable)?,
            has_steer_event: row.try_get(3).map_err(&unreadable)?,
            model_configured,
        })
    }

    /// Draws the identifier one upserted row carries.
    fn mint_id(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy.fill(&mut bytes)?;
        Ok(Uuid7::encode(now, bytes)?)
    }
}

/// Whether `bag` holds `key` set to the JSON literal `true`.
///
/// A key that is absent, or present holding anything else, is false. The
/// checklist has no third state: a step is ticked or it is not.
#[must_use]
pub fn bag_is_true(bag: &[Pref], key: PrefKey) -> bool {
    bag.iter()
        .find(|pref| pref.key == key.as_str())
        .is_some_and(|pref| pref.value.trim() == JSON_TRUE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registry_key_round_trips_through_its_wire_spelling() {
        for key in [
            PrefKey::GettingStartedDismissed,
            PrefKey::GettingStartedCollapsed,
            PrefKey::GettingStartedCliTicked,
        ] {
            assert_eq!(PrefKey::parse(key.as_str()), Some(key));
        }
    }

    #[test]
    fn a_key_outside_the_registry_is_refused() {
        // The whole point of the closed registry: an unbounded key space is an
        // unbounded number of rows per user, in a column nobody validates.
        assert_eq!(PrefKey::parse("getting_started"), None);
        assert_eq!(PrefKey::parse(""), None);
        assert_eq!(PrefKey::parse("GETTING_STARTED_DISMISSED"), None);
    }

    #[test]
    fn only_the_json_literal_true_reads_as_ticked() {
        let bag = |value: &str| {
            vec![Pref {
                key: PrefKey::GettingStartedCliTicked.as_str().to_owned(),
                value: value.to_owned(),
            }]
        };

        assert!(bag_is_true(&bag("true"), PrefKey::GettingStartedCliTicked));
        assert!(bag_is_true(
            &bag("  true  "),
            PrefKey::GettingStartedCliTicked
        ));
        // Everything a coercing implementation would accept, refused: the value
        // is stored verbatim, so these are what a client actually wrote.
        for written in ["false", "\"true\"", "1", "TRUE", "null", ""] {
            assert!(
                !bag_is_true(&bag(written), PrefKey::GettingStartedCliTicked),
                "{written} must not read as ticked"
            );
        }
    }

    #[test]
    fn an_absent_key_is_not_ticked() {
        assert!(!bag_is_true(&[], PrefKey::GettingStartedDismissed));
        assert!(!bag_is_true(
            &[Pref {
                key: PrefKey::GettingStartedCollapsed.as_str().to_owned(),
                value: JSON_TRUE.to_owned(),
            }],
            PrefKey::GettingStartedDismissed
        ));
    }
}
