//! A lease's static credentials, read the one way every caller reads them.
//!
//! `secrets_map` is free-form by design: `{"github": {"token": "…", "host":
//! "api.github.com"}}`. A credential is an object of string fields, and its
//! `host` field names where it may be sent rather than holding a secret. The
//! scrub masks every other string field; the egress guard substitutes them
//! and binds each credential to its host. Both read through this view, so the
//! shape is known in one place.

use std::fmt;

use serde_json::{Map, Value};

/// The credential field naming a credential's host, which is no secret.
pub const FIELD_HOST: &str = "host";

/// The credential field holding a credential's token: a minted credential's
/// only field, and the one a static credential's handler reads.
pub const FIELD_TOKEN: &str = "token";

/// The static credentials one lease carries.
///
/// Its `Debug` names the credentials and prints none of their fields, so a
/// lease or a tool context logged whole holds no secret.
#[derive(Clone, Copy, Default)]
pub struct StaticSecrets<'p> {
    credentials: Option<&'p Map<String, Value>>,
}

impl fmt::Debug for StaticSecrets<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self
            .credentials
            .into_iter()
            .flat_map(Map::keys)
            .map(String::as_str)
            .collect();
        f.debug_struct("StaticSecrets")
            .field("credentials", &names)
            .finish()
    }
}

impl<'p> StaticSecrets<'p> {
    /// The credentials in `secrets_map`; none when it is absent or not an
    /// object.
    #[must_use]
    pub fn new(secrets_map: Option<&'p Value>) -> Self {
        Self {
            credentials: secrets_map.and_then(Value::as_object),
        }
    }

    /// The credentials of a map already read as one, such as the declared
    /// half of a vault read: the same view, with no `Value` built around it.
    #[must_use]
    pub const fn of_map(credentials: &'p Map<String, Value>) -> Self {
        Self {
            credentials: Some(credentials),
        }
    }

    /// Whether a credential named `name` exists.
    #[must_use]
    pub fn contains(self, name: &str) -> bool {
        self.fields(name).is_some()
    }

    /// The string field `field` of credential `name`.
    #[must_use]
    pub fn field(self, name: &str, field: &str) -> Option<&'p str> {
        self.fields(name)?.get(field)?.as_str()
    }

    /// The host credential `name` may be sent to.
    #[must_use]
    pub fn host(self, name: &str) -> Option<&'p str> {
        self.field(name, FIELD_HOST)
    }

    /// Every secret value as `(name.field, value)`: each string field except
    /// `host`.
    pub fn values(self) -> impl Iterator<Item = (String, &'p str)> + 'p {
        self.credentials
            .into_iter()
            .flatten()
            .filter_map(|(name, credential)| Some((name, credential.as_object()?)))
            .flat_map(|(name, fields)| {
                fields
                    .iter()
                    .filter(|(field, _)| field.as_str() != FIELD_HOST)
                    .filter_map(move |(field, value)| {
                        Some((format!("{name}.{field}"), value.as_str()?))
                    })
            })
    }

    fn fields(self, name: &str) -> Option<&'p Map<String, Value>> {
        self.credentials?.get(name)?.as_object()
    }
}

#[cfg(test)]
#[path = "statics/tests.rs"]
mod tests;
