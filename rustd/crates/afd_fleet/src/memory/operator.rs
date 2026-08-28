//! What a PERSON does with a fleet's memory: read a page of it, or forget one.
//!
//! The runner's two verbs live in [`crate::lease::memory`] and write through
//! [`Memories::capture`]; these read and delete the same rows, which is why
//! they are methods on the same store rather than a second one over the same
//! table. The values they are parameterised by — which rows, where a page
//! resumes, what a row holds — are [`super::page`].
//!
//! # A tenant cannot author a memory, and can forget one
//!
//! There is no store verb here, deliberately. Memory is written by exactly one
//! path, the runner-plane capture push, so a fleet remembers what it LEARNED
//! and never what a caller asserted. Forgetting is the operator's correction: a
//! fleet that learned a convention wrong carries it into every hydrate until
//! somebody removes the entry, so the DELETE is the one mutation this surface
//! has.
//!
//! # Ownership is answered before the role switch, and that is the order
//!
//! `memory.memory_entries` carries no workspace column — it is scoped by
//! `fleet_id` alone — so "is this fleet this workspace's" has to be answered
//! against `core.fleets`, which the `memory_runtime` role cannot see.
//! [`Memories::in_workspace`] therefore runs FIRST, on the api role, before any
//! transaction takes the memory role, and a caller naming another workspace's
//! fleet never reaches the memory schema at all.
//!
//! Both public verbs take the workspace and do that check themselves. A method
//! that let a caller supply the fleet alone would be a method whose caller can
//! forget the check — the shape `helpers.zig` has, and the reason every one of
//! its handlers has to open with the same call.

use afd_core::id::Uuid7;
use sqlx::{Acquire as _, Row as _};

use super::page::{After, Entry, View};
use super::{Memories, sql};
use crate::error::{
    DETAIL_MEMORY_FORGET_FAILED, DETAIL_MEMORY_ROLE_SWITCH, Result, memory_entry_not_found,
    memory_fleet_not_found, memory_unavailable, query,
};

/// Statement name, for the context the ownership read's failure carries.
///
/// Its own context rather than one of the memory sentences, because it runs on
/// a different role against a different schema: a `core.fleets` read that fails
/// is not evidence the memory backend is down, and answering `UZ-MEM-003` for
/// it would send an operator to the wrong datastore.
const CONTEXT_OWNERSHIP: &str = "memory fleet ownership";

/// Statement name, for the context a refused column decode carries.
const CONTEXT_ROW: &str = "memory page row";

impl Memories {
    /// One page of `fleet`'s memory under `view`, newest first.
    ///
    /// # Errors
    /// Refuses a fleet `workspace` does not hold, reports a memory backend that
    /// would not answer, and reports a row this daemon cannot read.
    pub async fn page(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        view: View<'_>,
        after: Option<After<'_>>,
        limit: i64,
    ) -> Result<Vec<Entry>> {
        self.in_workspace(workspace, fleet).await?;
        let detail = view.detail();
        // Built before the pipeline so the pattern outlives every borrow the
        // query holds; a value computed inline would not.
        let filter = view.filter();

        let mut connection = self.database.acquire().await?;
        let mut transaction = connection
            .begin()
            .await
            .map_err(memory_unavailable(detail))?;
        sqlx::query(sql::ASSUME_MEMORY_ROLE)
            .execute(&mut *transaction)
            .await
            .map_err(memory_unavailable(DETAIL_MEMORY_ROLE_SWITCH))?;

        // One bind pipeline for all six statements. Their parameter order is
        // identical by construction — fleet, filter, boundary, limit — and
        // `sql.rs` says so where somebody editing one of them will read it.
        // The Zig spends three near-identical `fetch` functions on this, one
        // per view, each repeating the role switch and the drain.
        let mut statement = sqlx::query(view.statement(after.is_some())).bind(fleet.as_str());
        if let Some(value) = filter.as_deref() {
            statement = statement.bind(value);
        }
        if let Some(boundary) = after {
            statement = statement.bind(boundary.created_at_ms).bind(boundary.key);
        }
        let rows = statement
            .bind(limit)
            .fetch_all(&mut *transaction)
            .await
            .map_err(memory_unavailable(detail))?;

        // Collected before the commit, because the rows borrow the transaction.
        let entries = rows.iter().map(entry).collect::<Result<Vec<_>>>()?;
        transaction
            .commit()
            .await
            .map_err(memory_unavailable(detail))?;
        Ok(entries)
    }

    /// Removes one entry, and refuses a key the fleet is not holding.
    ///
    /// # Errors
    /// Refuses a fleet `workspace` does not hold, and a key with no row — which
    /// is a REFUSAL rather than a silent success, so a mistyped key is visible
    /// to whoever typed it. Reports a memory backend that would not answer.
    pub async fn forget(&self, workspace: &Uuid7, fleet: &Uuid7, key: &str) -> Result<()> {
        self.in_workspace(workspace, fleet).await?;

        let mut connection = self.database.acquire().await?;
        let mut transaction = connection
            .begin()
            .await
            .map_err(memory_unavailable(DETAIL_MEMORY_FORGET_FAILED))?;
        sqlx::query(sql::ASSUME_MEMORY_ROLE)
            .execute(&mut *transaction)
            .await
            .map_err(memory_unavailable(DETAIL_MEMORY_ROLE_SWITCH))?;

        let forgotten = sqlx::query(sql::DELETE_ENTRY_BY_KEY)
            .bind(fleet.as_str())
            .bind(key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(memory_unavailable(DETAIL_MEMORY_FORGET_FAILED))?;

        // Committed BEFORE the verdict, and the order is load-bearing: an
        // `ok_or_else` above this line would drop the transaction, and on the
        // deleted path that rolls the delete back — answering 204 for a row
        // that had just been removed and then was not.
        transaction
            .commit()
            .await
            .map_err(memory_unavailable(DETAIL_MEMORY_FORGET_FAILED))?;
        forgotten.map(|_row| ()).ok_or_else(memory_entry_not_found)
    }

    /// Proves `fleet` is `workspace`'s, or refuses.
    ///
    /// A fleet in another workspace and a fleet that does not exist answer the
    /// SAME refusal. Telling them apart would make this an oracle for which
    /// fleet identifiers are real, which is why `helpers.zig` answers 404 for
    /// both rather than a 403 for one.
    async fn in_workspace(&self, workspace: &Uuid7, fleet: &Uuid7) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        sqlx::query(sql::SELECT_FLEET_WORKSPACE)
            .bind(fleet.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_OWNERSHIP))?
            .map(|row| row.try_get::<String, _>(0))
            .transpose()
            .map_err(query(CONTEXT_OWNERSHIP))?
            .filter(|owning| owning == workspace.as_str())
            .map(|_owned| ())
            .ok_or_else(memory_fleet_not_found)
    }
}

/// One row as this surface reads it.
///
/// Every column answers [`crate::error::query`] rather than a memory-backend
/// failure, which is [`Memories::list`]'s handling of the same thing and the
/// honest one: the store DID answer, and what it handed back is the problem.
///
/// A refused column ends the page here. `collectEntries` instead returns what
/// it had collected and logs `collect_truncated`, so a 200 can carry fewer
/// entries than the fleet holds with nothing on the wire saying so — the
/// truncation its own comment warns about. Answering the failure is the
/// guarantee that workaround was reaching for, and it also deletes the `clean`
/// flag `handler.zig` threads through its recall-miss counter to keep a
/// truncated read from being counted as an empty one.
fn entry(row: &sqlx::postgres::PgRow) -> Result<Entry> {
    Ok(Entry {
        key: row.try_get(0).map_err(query(CONTEXT_ROW))?,
        content: row.try_get(1).map_err(query(CONTEXT_ROW))?,
        category: row.try_get(2).map_err(query(CONTEXT_ROW))?,
        updated_at_ms: row.try_get(3).map_err(query(CONTEXT_ROW))?,
        created_at_ms: row.try_get(4).map_err(query(CONTEXT_ROW))?,
    })
}
