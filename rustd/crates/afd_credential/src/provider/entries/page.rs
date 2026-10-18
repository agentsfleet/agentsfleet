//! The registry page: six reads, a fixed number of them, and no decrypt.
//!
//! # The statement count does not follow the page size
//!
//! One selection read, one entry page, one workspace resolve, one descriptor
//! batch, one platform default, one rate batch. Both batches are set-oriented,
//! so the count is independent of `limit` — and that INDEPENDENCE is the
//! property worth pinning, not the number. A per-row lookup is the unbounded
//! shape this composition exists to avoid.
//!
//! Decrypting is the other shape it avoids: opening an AES-GCM envelope per row
//! would cost a hundred-row page a hundred decryptions to render a view whose
//! every field is metadata. The `meta_*` columns carry what the view needs, and
//! [`Directory`] cannot decrypt at all.
//!
//! # One connection at a time, never two
//!
//! Each read below acquires and releases before the next begins. They are
//! independent and could be joined concurrently, which would be faster and
//! would also let one request hold three pool connections — the shape that
//! deadlocks a bounded pool under load. Sequential is the deliberate answer:
//! one connection held at a time.

use std::collections::HashMap;

use afd_core::id::Uuid7;
use afd_vault::{Descriptor, Directory};
use jiff_sqlx::ToSqlx as _;

use super::sql;
use super::{
    Boundary, CatalogueRate, Entry, EntryRow, PricedDefault, RegistryPage, RegistryRow, is_active,
};
use crate::error::{Result, query};
use crate::provider::cap;
use crate::provider::store::Providers;

/// The context a failed page read reports under.
const CONTEXT_PAGE: &str = "tenant model registry page";

/// One [`sql::SELECT_RATES_FOR_PAIRS`] row, by its column names.
#[derive(Debug, sqlx::FromRow)]
struct RateRow {
    provider: String,
    model_name: String,
    context_cap_tokens: i32,
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
}

impl Providers {
    /// One page of `tenant`'s registry, newest first.
    ///
    /// `after` is the decoded boundary from the caller's cursor, already
    /// checked against the authenticated tenant and the requested limit. This
    /// trusts it, because only the handler can perform that comparison.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a stored row this daemon
    /// cannot read, and a selection pointing at no readable entry. A credential
    /// deleted out of band is not an error — its row lists degraded.
    pub async fn registry_page(
        &self,
        tenant: &Uuid7,
        limit: u32,
        after: Option<&Boundary>,
    ) -> Result<RegistryPage> {
        let chosen = self.selection(tenant).await?;
        let (entries, next) = self.entry_page(tenant, limit, after).await?;
        let credentials = self.describe_referenced(tenant, &entries).await?;
        let default = self.platform_default().await?;

        let rates = self
            .priced(&entries, &credentials, default.as_ref())
            .await?;
        let rows = entries
            .into_iter()
            .map(|entry| {
                let credential = credentials.get(&entry.secret_ref).cloned();
                let rate = rate_for(&rates, credential.as_ref(), &entry.model_name);
                RegistryRow {
                    active: is_active(&entry, chosen.as_ref()),
                    credential,
                    rate,
                    entry,
                }
            })
            .collect();

        Ok(RegistryPage {
            rows,
            next,
            platform_default: default.map(|default| PricedDefault {
                rate: rates
                    .get(&(default.provider.to_string(), default.model.to_string()))
                    .copied(),
                default,
            }),
        })
    }

    /// The page's rows, and where a later page would resume.
    ///
    /// Fetches one row beyond the limit and never returns it: its existence is
    /// the whole answer to "is there another page?", and asking that with a
    /// `COUNT` would cost the scan keyset paging exists to avoid.
    ///
    /// The boundary is taken from the last SERVED row rather than from the
    /// extra one — the seek is exclusive, so it resumes strictly after what the
    /// caller has already seen.
    async fn entry_page(
        &self,
        tenant: &Uuid7,
        limit: u32,
        after: Option<&Boundary>,
    ) -> Result<(Vec<Entry>, Option<Boundary>)> {
        let fetch = i64::from(limit) + 1;
        let statement = match after {
            None => sqlx::query_as::<_, EntryRow>(sql::SELECT_FIRST_PAGE)
                .bind(tenant.as_str())
                .bind(fetch),
            Some(boundary) => sqlx::query_as::<_, EntryRow>(sql::SELECT_PAGE_AFTER)
                .bind(tenant.as_str())
                .bind(boundary.created_at.to_sqlx())
                .bind(boundary.id.as_str())
                .bind(fetch),
        };

        let mut connection = self.pool().acquire().await?;
        let rows = statement
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_PAGE))?;

        let has_more = rows.len() > limit as usize;
        let entries: Vec<Entry> = rows
            .into_iter()
            .take(limit as usize)
            .map(EntryRow::into_entry)
            .collect::<Result<_>>()?;

        // From the last SERVED row, never the extra one: the seek is exclusive,
        // so the next page has to resume after what the caller already saw.
        let next = has_more.then(|| entries.last().map(boundary_of)).flatten();
        Ok((entries, next))
    }

    /// What the vault says about every credential this page references.
    ///
    /// One workspace resolve for the whole page, not one per row: a tenant's
    /// entries all reference credentials in its primary workspace. A tenant
    /// with no workspace has no credentials to describe, so the page renders
    /// every row degraded rather than failing — the bootstrap invariant is
    /// activation's to refuse on, not a read's.
    async fn describe_referenced(
        &self,
        tenant: &Uuid7,
        entries: &[Entry],
    ) -> Result<HashMap<Box<str>, Descriptor>> {
        let Some(workspace) = self.primary_workspace(tenant).await? else {
            return Ok(HashMap::new());
        };
        // Deduplicated for the same reason the rate pairs are: one credential
        // legitimately backs several model rows, and the array is a question
        // asked once per distinct name rather than once per row that asks it.
        let mut names: Vec<&str> = entries.iter().map(|entry| &*entry.secret_ref).collect();
        names.sort_unstable();
        names.dedup();

        // Constructed here rather than held as a field: `Db` is a handle over an
        // `Arc`-backed pool, so this is one refcount bump, and a `Directory` has
        // no key to carry. What matters is WHICH type reads — one that cannot
        // decrypt — not how long it lives.
        Directory::new(self.pool().clone())
            .describe(&workspace, &names)
            .await
            .map_err(Into::into)
    }

    /// The catalogue's rates for every pair this page displays, in one read.
    ///
    /// A row whose credential is gone, or carries no provider, has no catalogue
    /// identity to ask about and contributes no pair — the same blank cell it
    /// already renders for every other descriptor field. The platform default's
    /// pair rides along, which is what keeps the page at one rate statement
    /// rather than two.
    async fn priced(
        &self,
        entries: &[Entry],
        credentials: &HashMap<Box<str>, Descriptor>,
        default: Option<&super::PlatformDefault>,
    ) -> Result<HashMap<(String, String), CatalogueRate>> {
        let pairs = wanted_pairs(entries, credentials, default);
        if pairs.is_empty() {
            return Ok(HashMap::new());
        }

        let (providers, models): (Vec<&str>, Vec<&str>) = pairs.into_iter().unzip();
        let mut connection = self.pool().acquire().await?;
        let rows = sqlx::query_as::<_, RateRow>(sql::SELECT_RATES_FOR_PAIRS)
            .bind(&providers)
            .bind(&models)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_PAGE))?;

        Ok(rows.into_iter().map(RateRow::into_rate).collect())
    }
}

/// Every `(provider, model)` pair the page needs a price for, deduplicated.
///
/// A set rather than one slot per row. One credential legitimately backs
/// several model rows and the platform default is usually also one of the
/// tenant's own, so the pairs repeat; asking for each repeat would send a
/// longer array to answer the same question. Duplicates are dropped here and
/// re-attached by lookup, which is what a map buys over positional slots
/// matched back by index.
fn wanted_pairs<'a>(
    entries: &'a [Entry],
    credentials: &'a HashMap<Box<str>, Descriptor>,
    default: Option<&'a super::PlatformDefault>,
) -> Vec<(&'a str, &'a str)> {
    let from_entries = entries.iter().filter_map(|entry| {
        let provider = credentials.get(&entry.secret_ref)?.provider()?;
        Some((provider, &*entry.model_name))
    });
    let from_default = default.map(|default| (&*default.provider, &*default.model));

    let mut pairs: Vec<(&str, &str)> = from_entries.chain(from_default).collect();
    pairs.sort_unstable();
    pairs.dedup();
    pairs
}

/// The rate for one row's pair, or nothing when either half is unknown.
///
/// The two `to_owned`s are the price of an owned map key, and they were weighed
/// rather than missed. `HashMap` has no way to look a tuple key up by a pair of
/// borrows — `Borrow` cannot span two fields, and `raw_entry` is unstable — so
/// the alternatives are this, a map borrowing from the rows the query returned
/// (which would leak `PgRow` into the page's composition), or the positional
/// slots this design exists to delete. Two short allocations per row, at most a
/// hundred rows, against six network round trips already spent.
fn rate_for(
    rates: &HashMap<(String, String), CatalogueRate>,
    credential: Option<&Descriptor>,
    model: &str,
) -> Option<CatalogueRate> {
    let provider = credential?.provider()?;
    rates.get(&(provider.to_owned(), model.to_owned())).copied()
}

/// Where a later page resumes after `entry`.
///
/// Built from the entry ROW rather than from anything projected beside it: the
/// seek predicate compares against `model.tenant_models` columns, and a
/// rendered view may not round-trip them.
fn boundary_of(entry: &Entry) -> Boundary {
    Boundary {
        created_at: entry.created_at,
        id: entry.id.clone(),
    }
}

impl RateRow {
    /// The row as its map entry.
    ///
    /// The ceiling is clamped the way every other reader of this column clamps
    /// it: `model.platform_models.context_cap_tokens` is `INTEGER NOT NULL`
    /// with no nonnegative constraint, because RULE STS keeps bounds in the
    /// application, so a negative row is one the schema permits and a page
    /// must not render.
    fn into_rate(self) -> ((String, String), CatalogueRate) {
        (
            (self.provider, self.model_name),
            CatalogueRate {
                context_cap_tokens: cap::stored(self.context_cap_tokens),
                input_nanos_per_mtok: self.input_nanos_per_mtok,
                cached_input_nanos_per_mtok: self.cached_input_nanos_per_mtok,
                output_nanos_per_mtok: self.output_nanos_per_mtok,
            },
        )
    }
}
