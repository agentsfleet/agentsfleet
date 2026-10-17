//! Every statement this crate runs, and nothing else.
//!
//! Each is a `.sql` file beside this module, schema-qualified (RULE NSQ), and
//! checked against the rebuilt schema by `afd_db`'s
//! `test_every_sql_file_prepares`. Every write stamps `updated_at` with the
//! database's `now()` (`docs/architecture/postgres_schemas.md` rule 4).

/// Every priced model, in stable provider/model order.
pub(crate) const LIST_PLATFORM_MODELS: &str = include_str!("list_platform_models.sql");

/// Files a model, or writes nothing when `(provider, model_name)` is taken.
///
/// `$1` id · `$2` provider · `$3` model name · `$4` context cap · `$5`–`$7` the
/// three prices.
pub(crate) const INSERT_PLATFORM_MODEL: &str = include_str!("insert_platform_model.sql");

/// Replaces a model's cap and prices. `$1` id · `$2` cap · `$3`–`$5` prices.
pub(crate) const UPDATE_PLATFORM_MODEL: &str = include_str!("update_platform_model.sql");

/// Removes a model. A platform default still pointing at it refuses the delete
/// through `ON DELETE RESTRICT`, which [`crate::Models::delete`] reports as
/// in use. `$1` id.
pub(crate) const DELETE_PLATFORM_MODEL: &str = include_str!("delete_platform_model.sql");

/// Advances the catalogue's edit counter.
///
/// Run FIRST in every catalogue edit: the row lock it takes serializes
/// concurrent edits, and an edit that turns out to change nothing rolls back
/// and takes the bump with it.
pub(crate) const BUMP_CATALOGUE_VERSION: &str = include_str!("bump_catalogue_version.sql");

/// Every platform default with its model's name, in provider order.
pub(crate) const LIST_PLATFORM_DEFAULTS: &str = include_str!("list_platform_defaults.sql");

/// Activates a provider's default on a catalogued model.
///
/// The model is selected, never bound: a `(provider, model)` pair the catalogue
/// does not carry inserts nothing and returns no row. A source workspace with
/// no row refuses through its foreign key.
///
/// `$1` provider · `$2` source workspace · `$3` model name · `$4` base URL.
pub(crate) const UPSERT_PLATFORM_DEFAULT: &str = include_str!("upsert_platform_default.sql");

/// Deactivates every default except `$1`'s, keeping one active platform-wide.
pub(crate) const DEACTIVATE_OTHER_PLATFORM_DEFAULTS: &str =
    include_str!("deactivate_other_platform_defaults.sql");

/// Deactivates `$1`'s default and releases its model.
pub(crate) const DEACTIVATE_PLATFORM_DEFAULT: &str =
    include_str!("deactivate_platform_default.sql");
