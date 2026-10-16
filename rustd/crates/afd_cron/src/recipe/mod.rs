//! A pinned base and an explicit instance overlay are versioned independently.

mod guard;
mod replace;
mod retire;
mod sql;
mod write;

#[derive(sqlx::FromRow)]
struct Stored {
    id: String,
    slot: i64,
    recipe: Option<String>,
    held: bool,
}

use crate::{Result, error::ErrorKind, validate};
use afd_api_wire::schedule::{Override, Status};
use afd_core::{clock::UnixMillis, id::Uuid7};
use serde::{Deserialize, Serialize};

/// Immutable schedule fields copied from the selected native setup revision.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub(crate) cron: String,
    pub(crate) timezone: String,
    pub(crate) message: String,
    pub(crate) status: Status,
}

impl Recipe {
    /// Bounds and validates fields before creating a recipe.
    ///
    /// # Errors
    /// Rejects unsupported expressions, timezone names and empty/oversized text.
    pub fn parse(cron: &str, timezone: &str, message: &str) -> Result<Self> {
        validate::Fields {
            expression: Some(cron),
            timezone: Some(timezone),
            message: Some(message),
        }
        .check()
        .map_err(|invalid| ErrorKind::Invalid { invalid })?;
        Ok(Self {
            cron: cron.to_owned(),
            timezone: timezone.to_owned(),
            message: message.to_owned(),
            status: Status::Active,
        })
    }
}

/// Stored inheritance survives Library changes and makes Restore deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Inherited {
    /// Position in the pinned Definition, independent of the upstream key.
    pub(crate) slot: i64,
    pub(crate) base: Recipe,
    /// Changes only when the inherited base changes.
    pub recipe_revision: i64,
    /// Selected fleet setup that supplied this base.
    pub setup_revision: i64,
    /// Changes whenever a person replaces or clears an overlay.
    pub override_revision: i64,
    /// Instance-specific field replacements.
    #[serde(rename = "override", skip_serializing_if = "Option::is_none")]
    pub overrides: Option<Override>,
}

impl Inherited {
    pub(crate) fn effective(&self) -> Recipe {
        let Some(overlay) = &self.overrides else {
            return self.base.clone();
        };
        Recipe {
            cron: overlay
                .cron
                .clone()
                .unwrap_or_else(|| self.base.cron.clone()),
            timezone: overlay
                .timezone
                .clone()
                .unwrap_or_else(|| self.base.timezone.clone()),
            message: overlay
                .message
                .clone()
                .unwrap_or_else(|| self.base.message.clone()),
            status: overlay.status.unwrap_or(self.base.status),
        }
    }
}

/// Bases copied by one setup revision on the caller's transaction.
#[derive(Debug, Clone, Copy)]
pub struct RecipeSet<'a> {
    /// Workspace whose parent fleet must be locked.
    pub workspace: &'a Uuid7,
    /// Independent runtime instance.
    pub fleet: &'a Uuid7,
    /// New immutable setup revision.
    pub setup_revision: i64,
    /// Validated cron recipes in Definition order.
    pub recipes: &'a [Recipe],
    /// Audit time of this setup edit.
    pub now: UnixMillis,
}
