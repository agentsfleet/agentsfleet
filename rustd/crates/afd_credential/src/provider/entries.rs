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
//! live here, and `core.tenant_model_entries` already has a writer here —
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

use afd_core::id::Uuid7;
use afd_vault::Descriptor;

use super::selection::{PlatformDefault, Selection};

/// One `core.tenant_model_entries` row, as every verb here answers with it.
///
/// No `tenant_id`: it is the predicate every statement filters on, never a
/// column a caller reads back. No `updated_at`: nothing renders it, and a field
/// no reader consumes is one a future edit has to keep correct for nobody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The entry's own minted identity — the `{id}` in the item route.
    pub id: Uuid7,
    /// The model this entry configures.
    pub model_id: Box<str>,
    /// The vault key name backing it. Immutable once the entry exists.
    pub secret_ref: Box<str>,
    /// When it was first stored, in epoch milliseconds.
    pub created_at_ms: i64,
}

/// What the catalogue charges for one `(provider, model)` pair, for display.
///
/// Not `afd_tenant`'s catalogue row and not the billing meter's slice rates —
/// neither is reachable from this crate, and neither is this: it carries the
/// row's neither identity nor version, because a registry row SHOWS a price and
/// never quotes one. A rate the catalogue does not carry is `None` at the call site
/// rather than a zero here — a blank cell and a free model must not render the
/// same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogueRate {
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
    /// Computed by comparing `(secret_ref, model_id)` against the selection
    /// row, because no `active` column exists — the registry records what a
    /// tenant may run on, and the selection records what it does run on.
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
    pub created_at_ms: i64,
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
    /// It is the tenant's active selection, so removing it would leave the
    /// selection naming a row that does not exist.
    Active,
}

/// Whether `entry` is the row `selection` currently runs on.
///
/// No `active` column exists, so the comparison is by `(secret_ref, model_id)`
/// — the same pair the domain key is built from, and the same one the delete
/// path refuses on. A platform-mode selection matches nothing here: it names no
/// credential, and an entry always does.
fn is_active(entry: &Entry, selection: Option<&Selection>) -> bool {
    selection.is_some_and(|chosen| {
        chosen.secret_ref.as_deref() == Some(&*entry.secret_ref) && *chosen.model == *entry.model_id
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Entry, is_active};
    use crate::provider::selection::Selection;
    use afd_billing::Posture;
    use afd_core::id::Uuid7;

    /// A fixed identity, so two entries in one test are distinguishable.
    const ENTRY_ID: &str = "0195b4ba-8d3a-7f13-8abc-cd0000000002";

    fn entry(model: &str, secret: &str) -> Entry {
        Entry {
            id: Uuid7::parse(ENTRY_ID).expect("the fixture id is a canonical uuidv7"),
            model_id: model.into(),
            secret_ref: secret.into(),
            created_at_ms: 1_744_000_000_000,
        }
    }

    fn selection(posture: Posture, model: &str, secret: Option<&str>) -> Selection {
        Selection {
            posture,
            provider: "anthropic".into(),
            model: model.into(),
            context_cap_tokens: 200_000,
            secret_ref: secret.map(Into::into),
        }
    }

    #[test]
    fn the_active_row_is_the_one_matching_both_halves_of_the_pair() {
        let row = entry("claude-opus-5", "anthropic-prod");
        let chosen = selection(
            Posture::SelfManaged,
            "claude-opus-5",
            Some("anthropic-prod"),
        );
        assert!(is_active(&row, Some(&chosen)));
    }

    #[test]
    fn half_a_match_is_not_a_match() {
        // Both halves matter: the same model on a different credential, and a
        // different model on the same credential, are different entries — which
        // is what the table's domain key says.
        let row = entry("claude-opus-5", "anthropic-prod");
        for chosen in [
            selection(Posture::SelfManaged, "claude-opus-5", Some("anthropic-dev")),
            selection(
                Posture::SelfManaged,
                "claude-sonnet-5",
                Some("anthropic-prod"),
            ),
        ] {
            assert!(!is_active(&row, Some(&chosen)));
        }
    }

    #[test]
    fn a_tenant_on_the_platform_default_has_no_active_entry() {
        // A platform selection names no credential, and every entry does, so
        // nothing on the page is active. The page still renders — the Default
        // row is what carries the highlight.
        let row = entry("claude-opus-5", "anthropic-prod");
        let chosen = selection(Posture::Platform, "claude-opus-5", None);
        assert!(!is_active(&row, Some(&chosen)));
        assert!(!is_active(&row, None));
    }
}
