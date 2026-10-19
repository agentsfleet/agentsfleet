//! `/v1/tenants/me/models` — the many models a tenant may choose between.
//!
//! [`super::selection`] holds the ONE row a tenant's runs resolve through.
//! This is the registry it is chosen from: many rows per tenant, each pairing a
//! model with a stored credential, each rendered with what the vault says about
//! that credential and what the catalogue charges for that model.
//!
//! # Why it lives beside provider resolution rather than in its own crate
//!
//! The registry's `active` flag is computed against [`Selection`], its page
//! carries the live [`PlatformDefault`], and its writes reach the credential
//! through the same primary-workspace bridge activation uses. All three already
//! live here, and `model.tenant_models` already has a writer here —
//! activation guarantees an entry for the pair it selects. Splitting the
//! readers out would put one table's statements in two crates, which is exactly
//! what collecting them in a `sql` module exists to prevent.
//!
//! # The refusals are values
//!
//! Every way a client can be told no — a credential nobody stored, a pair the
//! tenant already has, an id that does not resolve, an entry that is the active
//! selection — comes back as an outcome variant rather than an error, the same
//! discipline [`Activation`](super::Activation) follows. Each is a decision
//! made from a value this module already holds, so the handler answers a
//! registry code from the fact itself instead of matching on a datastore
//! failure's neighbours. Genuine faults still travel as `Err`.
//!
//! # Nothing here decrypts
//!
//! The page renders provider, kind, endpoint and key PRESENCE, all of which are
//! `meta_*` columns beside the ciphertext rather than inside it. The read goes
//! through [`afd_vault::Directory`], which holds no key — so the guarantee is a
//! property of the type doing the reading, not of a path a reviewer has to
//! follow.

mod page;
pub mod sql;
mod write;

use afd_core::catalogue::Metadata;
use afd_core::id::Uuid7;
use afd_vault::Descriptor;
use jiff::Timestamp;

use super::selection::{PlatformDefault, Selection};
use crate::error::{Result, row_malformed};

/// The table an unreadable identifier is reported against.
const TABLE_ENTRIES: &str = "model.tenant_models";

/// The column an unreadable identifier is reported against.
const COLUMN_ID: &str = "id";

/// One `model.tenant_models` row, as every verb here answers with it.
///
/// No `tenant_id`: it is the predicate every statement filters on, never a
/// column a caller reads back. No `updated_at`: nothing renders it, and a field
/// no reader consumes is one a future edit has to keep correct for nobody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The entry's own minted identity — the `{id}` in the item route.
    pub id: Uuid7,
    /// The model this entry configures — the provider's name for it.
    pub model_name: Box<str>,
    /// The vault key name backing it. Immutable once the entry exists.
    pub secret_ref: Box<str>,
    /// When it was first stored.
    pub created_at: Timestamp,
}

/// One registry row as every statement in [`sql`] names its columns.
#[derive(Debug, sqlx::FromRow)]
struct EntryRow {
    id: String,
    model_name: String,
    secret_ref: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
}

impl EntryRow {
    /// The row, typed.
    ///
    /// # Errors
    /// Reports an `id` that is not an identifier.
    fn into_entry(self) -> Result<Entry> {
        Ok(Entry {
            id: Uuid7::parse(&self.id).map_err(row_malformed(TABLE_ENTRIES, COLUMN_ID))?,
            model_name: self.model_name.into_boxed_str(),
            secret_ref: self.secret_ref.into_boxed_str(),
            created_at: self.created_at,
        })
    }
}

/// What the catalogue charges for one `(provider, model)` pair, for display.
///
/// Not `afd_tenant`'s catalogue row and not the billing meter's slice rates —
/// neither is reachable from this crate. This projection carries catalogue
/// display/context evidence and a current base quote, without its row id or
/// version. A rate the catalogue does not carry is `None` at the call site
/// rather than a zero here — a blank cell and a free model must not render the
/// same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueRate {
    /// Verified catalogue identity and display label for the exact route.
    pub metadata: Metadata,
    /// The context window the price is quoted for.
    pub context_cap_tokens: u32,
    /// The input rate, in nanos per million tokens.
    pub input_nanos_per_mtok: i64,
    /// The cached-input rate, likewise.
    pub cached_input_nanos_per_mtok: i64,
    /// The output rate, likewise.
    pub output_nanos_per_mtok: i64,
}

/// One entry, joined to everything the page shows beside it.
///
/// `credential` is `None` for an entry whose vault row was deleted out of band.
/// That still lists — degraded to an opaque secret with no key — because a page
/// of twenty models must not fail over one dangling reference. The same
/// per-row resilience the workspace secret list has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryRow {
    /// The stored row.
    pub entry: Entry,
    /// What the vault says about the credential it names.
    pub credential: Option<Descriptor>,
    /// What the catalogue charges for it.
    pub rate: Option<CatalogueRate>,
    /// Whether this entry IS the tenant's current self-managed selection.
    ///
    /// Computed by comparing the selection's pointer with the entry's id,
    /// because no `active` column exists — the registry records what a tenant
    /// may run on, and the selection records what it does run on.
    pub active: bool,
}

/// The deployment's platform default, priced for the page that shows it.
///
/// A tenant sees it whether or not it runs on it: the Models page renders the
/// Default row's model and context from here, and gates its "switch back"
/// action on this being present at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricedDefault {
    /// The active default's identity.
    pub default: PlatformDefault,
    /// What the catalogue charges for it, folded into the page's one rate read.
    pub rate: Option<CatalogueRate>,
}

/// Where a later page resumes from.
///
/// The sort key and the tiebreak, and nothing else. The token a client actually
/// holds carries more — the tenant and the page size it was issued under — but
/// binding a cursor to its query is a decision only the handler can make and
/// check, so the store answers the boundary and stays out of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boundary {
    /// The boundary row's creation instant.
    pub created_at: Timestamp,
    /// The boundary row's id, which breaks ties within a millisecond.
    pub id: Uuid7,
}

/// One page of the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPage {
    /// The rows, newest first.
    pub rows: Vec<RegistryRow>,
    /// Where the next page resumes, or nothing on the last one.
    pub next: Option<Boundary>,
    /// The live platform default, priced — absent when no operator has set one.
    ///
    /// Whether it is present is the same fact the page's
    /// `platform_default_available` flag reports, read once so the two cannot
    /// disagree.
    pub platform_default: Option<PricedDefault>,
}

/// What adding an entry resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Added {
    /// Stored. Carries the row as written, for the response to echo.
    Stored(Entry),
    /// No credential is held under that name in the tenant's workspace.
    ///
    /// Covers "never existed" and "deleted a moment ago" alike. Both mean the
    /// same thing to the caller and neither is repaired by re-sending, so the
    /// answer is the 404 rather than the conflict a lost race would suggest.
    CredentialMissing,
    /// The tenant already has this exact model on this exact credential.
    Duplicate,
    /// This tenant has no workspace at all — a violated bootstrap invariant.
    NoWorkspace,
}

/// What pointing an entry at another model resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Retargeted {
    /// Stored. Carries the row as written.
    Stored(Entry),
    /// No entry with that id belongs to this tenant.
    NotFound,
    /// The tenant already has that model on this entry's credential.
    Duplicate,
}

/// What removing an entry resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removed {
    /// Gone — whether this call removed it or it was already absent.
    ///
    /// Idempotent by design: a caller retrying a delete it never saw the
    /// response to must not be told the row is missing, and there is nothing
    /// for it to do differently if it were.
    Done,
    /// It is the tenant's active selection, so removing it would silently
    /// point the tenant back at the platform default.
    Active,
}

/// Whether `entry` is the row `selection` currently runs on.
///
/// The selection is a pointer at one entry, so this is identity, not a match
/// on the pair the entry carries. A platform-mode selection points at nothing,
/// so nothing on the page is active under it.
fn is_active(entry: &Entry, selection: Option<&Selection>) -> bool {
    matches!(selection, Some(Selection::SelfManaged(own)) if own.tenant_model_id == entry.id)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Entry, EntryRow, is_active};
    use crate::provider::selection::{OwnModel, Selection};
    use afd_core::id::Uuid7;
    use jiff::Timestamp;

    /// A fixed identity, so two entries in one test are distinguishable.
    const ENTRY_ID: &str = "0195b4ba-8d3a-7f13-8abc-cd0000000002";

    /// Another, for the entry the selection does not point at.
    const OTHER_ID: &str = "0195b4ba-8d3a-7f13-8abc-cd0000000003";

    /// When the fixture entry was stored.
    const CREATED: Timestamp = Timestamp::constant(1_744_000_000, 0);

    fn id(raw: &str) -> Uuid7 {
        Uuid7::parse(raw).expect("the fixture id is a canonical uuidv7")
    }

    fn entry(raw: &str) -> Entry {
        Entry {
            id: id(raw),
            model_name: "claude-opus-5".into(),
            secret_ref: "anthropic-prod".into(),
            created_at: CREATED,
        }
    }

    fn pointing_at(raw: &str) -> Selection {
        Selection::SelfManaged(OwnModel {
            tenant_model_id: id(raw),
            model: "claude-opus-5".into(),
            secret_ref: "anthropic-prod".into(),
            provider: Some("anthropic".into()),
            context_cap_tokens: 200_000,
        })
    }

    #[test]
    fn the_active_row_is_the_one_the_selection_points_at() {
        assert!(is_active(&entry(ENTRY_ID), Some(&pointing_at(ENTRY_ID))));
    }

    #[test]
    fn the_same_pair_on_another_row_is_not_the_active_one() {
        // Identity, not the pair: an entry carrying the same model and
        // credential under another id is a different row.
        assert!(!is_active(&entry(OTHER_ID), Some(&pointing_at(ENTRY_ID))));
    }

    #[test]
    fn a_tenant_on_the_platform_default_has_no_active_entry() {
        // A platform selection points at nothing, so nothing on the page is
        // active. The page still renders — the Default row carries the
        // highlight.
        let row = entry(ENTRY_ID);
        assert!(!is_active(&row, Some(&Selection::Platform)));
        assert!(!is_active(&row, None));
    }

    #[test]
    fn a_stored_row_that_is_not_an_identifier_is_refused() {
        let row = EntryRow {
            id: "not-a-uuid".to_owned(),
            model_name: "claude-opus-5".to_owned(),
            secret_ref: "anthropic-prod".to_owned(),
            created_at: CREATED,
        };
        row.into_entry()
            .expect_err("a malformed id is a fault, not an entry");
    }
}
