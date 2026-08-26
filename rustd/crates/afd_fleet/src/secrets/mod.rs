//! The credentials a fleet declared, split by how the runner will get them.
//!
//! The stored config carries a list of credential NAMES. This resolves each to
//! the vault object behind it and then splits the set in two: a static
//! credential ships its stored value in `secrets_map`, where the runner's tool
//! bridge reads it as `${secrets.<name>.<field>}`; an on-demand one ships as an
//! id and a name only, and the runner comes back to the broker for a
//! short-lived token.
//!
//! # The split is a TYPE, which is what makes the invariant hold
//!
//! `runBilling` resolves every credential into one list and the lease body then
//! walks it, calling `mintableId` per entry and appending to one of two
//! builders. Nothing stops a future edit from appending a mintable handle to
//! both, and Invariant 1 — a stored App config never reaches the child — is
//! held by that walk being written correctly.
//!
//! Here the walk produces a [`Declared`] with two fields, and it is the only
//! thing that can produce one. A credential lands in exactly one of them
//! because a single `match` decides which, and neither field is constructible
//! from outside this module. The invariant is not enforced by review of the
//! caller; there is no caller that could break it.
//!
//! # What a mintable entry deliberately does not carry
//!
//! Its handle. Not a redacted handle, not the fields the broker will need — the
//! name and the id, and the broker re-reads the vault itself when the runner
//! asks. That is why [`Mintable`] has two fields and no third.

pub mod connector;

use afd_core::error_code;
use afd_core::id::Uuid7;
use serde_json::{Map, Value};

use crate::error::{Result, credential_missing, vault_data_invalid};
use crate::vault::{Held, Vault};

pub use self::connector::{Connector, Connectors, Descriptor, Registry, Supply};

/// A fleet declared a credential the vault does not hold.
const EVENT_CREDENTIAL_NOT_FOUND: &str = "credential_not_found";

/// One credential the runner must mint rather than receive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mintable {
    /// The name the fleet declared it under.
    pub name: Box<str>,
    /// Which connector mints it, by the name the broker resolves.
    ///
    /// The connector's NAME rather than a handle to the connector itself: this
    /// value is what travels on the execution policy, and the broker resolves
    /// it again through its own registry when the runner asks. A borrowed
    /// `&dyn Connector` would tie the whole lease to the registry's lifetime
    /// for a string it is about to serialize.
    pub integration: Box<str>,
}

/// Every credential a fleet declared, routed.
///
/// The two fields are disjoint by construction — see the module note. Order in
/// both follows the DECLARED order rather than the order Postgres returned the
/// rows in, because a fleet author reading a lease should see their own list
/// back.
#[derive(Clone, Default)]
pub struct Declared {
    /// Credentials whose stored value ships to the runner.
    ///
    /// A `serde_json::Map`, which this workspace takes with `preserve_order`,
    /// so this is an insertion-ordered map rather than an alphabetised one.
    ///
    /// # This holds live tenant credentials
    ///
    /// It is the one value in the lease that does, and it is why [`Declared`]
    /// has a hand-written [`Debug`]. The stored objects are not wiped on drop
    /// the way a provider key is — they are bound for the wire, and a
    /// `serde_json::Value` tree has nowhere to put a destructor — so what is
    /// defended here is the realistic leak: one `tracing` field spelled
    /// `?declared` putting every tenant credential into a log stream.
    secrets_map: Map<String, Value>,
    /// Credentials the runner comes back for, as an id and a name.
    mintable: Vec<Mintable>,
}

impl Declared {
    /// The stored values, for the execution policy.
    #[must_use]
    pub const fn secrets_map(&self) -> &Map<String, Value> {
        &self.secrets_map
    }

    /// The credentials the runner must mint.
    #[must_use]
    pub fn mintable(&self) -> &[Mintable] {
        &self.mintable
    }
}

/// Renders the shape, never the credentials.
///
/// The mintable half renders in full — a name and a connector id are not
/// secret, and they are what an operator debugging a mint needs to see. The map
/// renders as its NAMES and a count, because the names are the fleet's own
/// declarations and the values under them are the tenant's secrets.
impl core::fmt::Debug for Declared {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Declared")
            .field("secrets_map", &self.secrets_map.keys().collect::<Vec<_>>())
            .field("mintable", &self.mintable)
            .finish()
    }
}

impl Vault {
    /// Resolve `names` in `workspace_id` and route each to its channel.
    ///
    /// ONE vault read for the whole set. The Zig's per-name loop cost a round
    /// trip per declared credential, which for a fleet declaring six is six
    /// times the latency for a set the statement can fetch at once.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a declared name the vault
    /// does not hold, and a stored body that is not a JSON object. All three
    /// end the event: a fleet must never run with a credential it declared and
    /// cannot read, because the tool that needs it will fail mid-run instead —
    /// after the work has been billed.
    pub async fn declared(
        &self,
        workspace_id: &Uuid7,
        names: &[&str],
        connectors: &dyn Connectors,
    ) -> Result<Declared> {
        let held = self.open_many(workspace_id, names).await?;
        names
            .iter()
            .try_fold(Declared::default(), |mut declared, name| {
                let body = find(&held, name)
                    .ok_or_else(|| missing(workspace_id, name))?
                    .plaintext
                    .expose();
                let stored: Value =
                    serde_json::from_slice(body).map_err(|_shape| vault_data_invalid())?;
                declared.route((*name).to_owned(), stored, connectors)?;
                Ok(declared)
            })
    }
}

impl Declared {
    /// Send one resolved credential to whichever channel carries it.
    ///
    /// The single branch the module note is about: a credential reaches
    /// exactly one field because exactly one arm runs, and a mintable one's
    /// stored handle is DROPPED here rather than carried forward — which is
    /// Invariant 1 as an ownership fact rather than as a rule about what to
    /// append where.
    fn route(&mut self, name: String, stored: Value, connectors: &dyn Connectors) -> Result<()> {
        if let Some(connector) = connector::mintable(connectors, &stored) {
            // `stored` is not moved into anything here: the handle is dropped
            // at the end of this branch, which is Invariant 1.
            self.mintable.push(Mintable {
                name: name.into_boxed_str(),
                integration: connector.name().into(),
            });
        } else {
            // A stored value the tool bridge addresses by field, so it has to
            // be an object. `parseSecretJson` applies the same gate, and it is
            // the gate that keeps `${secrets.name.field}` from resolving
            // against a bare string.
            if !stored.is_object() {
                return Err(vault_data_invalid());
            }
            self.secrets_map.insert(name, stored);
        }
        Ok(())
    }
}

#[cfg(test)]
impl Declared {
    /// A declaration carrying mintable entries and no stored values.
    ///
    /// The routing above needs a vault read to reach; the policy assembly's
    /// grant pass needs only the mintable half. This hands a sibling unit test
    /// that half directly rather than making it stand up a datastore to prove
    /// a decision no datastore takes part in.
    pub(crate) fn with_mintable<'a>(entries: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Self {
            secrets_map: Map::new(),
            mintable: entries
                .into_iter()
                .map(|(name, integration)| Mintable {
                    name: name.into(),
                    integration: integration.into(),
                })
                .collect(),
        }
    }
}

/// The credential stored under `name`, if the batch read returned one.
///
/// Linear, mirroring the Zig's `findEntry` and for its reason: the declared set
/// is bounded by what a fleet author wrote in one file, and comparing a handful
/// of names is cheaper than building a map to look them up in.
fn find<'a>(held: &'a [Held], name: &str) -> Option<&'a Held> {
    held.iter().find(|entry| &*entry.name == name)
}

/// Reports a declared credential the vault does not hold.
fn missing(workspace_id: &Uuid7, name: &str) -> crate::Error {
    let code = error_code::AGENTSFLEET_CREDENTIAL_MISSING.as_str();
    let workspace = workspace_id.as_str();
    tracing::warn!(
        error_code = code,
        event = EVENT_CREDENTIAL_NOT_FOUND,
        workspace_id = workspace,
        name,
        "the fleet declared a credential this workspace does not hold"
    );
    credential_missing()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Declared, Registry};
    use serde_json::json;

    /// Routes `stored` under `name`, which is the whole of what the batch read
    /// hands the split — so the split is provable with no vault.
    fn routed(entries: &[(&str, serde_json::Value)]) -> Declared {
        let mut declared = Declared::default();
        for (name, stored) in entries {
            declared
                .route((*name).to_owned(), stored.clone(), &Registry)
                .expect("a well-formed credential routes");
        }
        declared
    }

    #[test]
    fn a_static_credential_ships_its_stored_value() {
        let declared = routed(&[("fly", json!({"api_token": "FlyTokenXyz"}))]);

        assert_eq!(
            declared.secrets_map().get("fly"),
            Some(&json!({"api_token": "FlyTokenXyz"}))
        );
        assert!(declared.mintable().is_empty());
    }

    #[test]
    fn a_mintable_credentials_handle_never_reaches_the_map() {
        // Invariant 1, and the reason the split is a type: the stored App
        // config is dropped by the arm that classifies it, so there is no
        // later step that has to remember not to carry it.
        let declared = routed(&[(
            "gh",
            json!({"integration": "github", "installation_id": "42", "app_id": "7"}),
        )]);

        assert!(
            declared.secrets_map().is_empty(),
            "a mintable handle reached the map: {:?}",
            declared.secrets_map()
        );
        assert_eq!(
            declared.mintable(),
            [super::Mintable {
                name: "gh".into(),
                integration: "github".into(),
            }]
        );
    }

    #[test]
    fn the_two_channels_stay_disjoint_across_a_mixed_set() {
        let declared = routed(&[
            ("fly", json!({"api_token": "t"})),
            ("gh", json!({"integration": "github"})),
            ("pat", json!({"integration": "static", "token": "ghp"})),
            (
                "zoho",
                json!({"integration": "zoho", "refresh_token": "rt"}),
            ),
        ]);

        let stored: Vec<_> = declared.secrets_map().keys().collect();
        assert_eq!(stored, ["fly", "pat"], "declaration order is preserved");
        let minted: Vec<_> = declared
            .mintable()
            .iter()
            .map(|entry| &*entry.name)
            .collect();
        assert_eq!(minted, ["gh", "zoho"]);
        assert!(
            minted
                .iter()
                .all(|name| !declared.secrets_map().contains_key(*name)),
            "a credential reached both channels"
        );
    }

    #[test]
    fn a_static_credential_that_is_not_an_object_is_refused() {
        // The tool bridge addresses these by field. A bare string has no
        // fields, so `${secrets.name.field}` would resolve against nothing —
        // and it would do so at tool-call time, mid-run, after the work was
        // billed.
        for body in [json!("just-a-token"), json!(["a", "b"]), json!(42)] {
            let mut declared = Declared::default();
            declared
                .route("cred".to_owned(), body.clone(), &Registry)
                .expect_err("a non-object stored credential cannot be addressed");
        }
    }

    #[test]
    fn a_declaration_never_renders_its_stored_values() {
        let declared = routed(&[
            ("fly", json!({"api_token": "FlyTokenXyz"})),
            ("gh", json!({"integration": "github"})),
        ]);
        let rendered = format!("{declared:?}");

        assert!(!rendered.contains("FlyTokenXyz"), "{rendered}");
        // The names and the mintable half DO render — an operator debugging a
        // mint needs both, and neither is a secret.
        assert!(rendered.contains("fly"));
        assert!(rendered.contains("github"));
    }
}
