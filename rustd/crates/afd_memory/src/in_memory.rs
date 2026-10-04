//! A store held in this process, behind `test-util`, that the flip is proved
//! against.
//!
//! Lock-free like the route table: the rows are one map behind an
//! [`ArcSwap`], replaced whole by each write through `rcu`. It keeps every
//! entry — no retention sweep and no cap — because what it stands in for is a
//! store's identity and newer-row rules, never Postgres's housekeeping.

use std::collections::HashMap;
use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::memory::MemoryDelta;
use arc_swap::ArcSwap;

use crate::error::Result;
use crate::page::{After, View};
use crate::record::{Housekept, Owner, Record};
use crate::store::MemoryStore;

/// One row: its workspace, and the entry. Keyed by `(writer, key)`, the same
/// identity the Postgres table's unique constraint holds.
#[derive(Debug, Clone)]
struct Row {
    workspace: Uuid7,
    record: Record,
}

type Rows = HashMap<(Uuid7, String), Row>;

/// Memory held in this process.
#[derive(Debug)]
pub struct InMemory {
    name: &'static str,
    rows: ArcSwap<Rows>,
}

impl InMemory {
    /// An empty store logged as `name`.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            rows: ArcSwap::from_pointee(HashMap::new()),
        }
    }

    /// The rows `keep` selects, sorted by `order`.
    fn select(
        &self,
        keep: impl Fn(&Row) -> bool,
        order: impl Fn(&Record, &Record) -> std::cmp::Ordering,
    ) -> Vec<Record> {
        let mut chosen: Vec<Record> = self
            .rows
            .load()
            .values()
            .filter(|row| keep(row))
            .map(|row| row.record.clone())
            .collect();
        chosen.sort_by(order);
        chosen
    }
}

/// Newest write first.
fn newest(left: &Record, right: &Record) -> std::cmp::Ordering {
    right.updated_at_ms.cmp(&left.updated_at_ms)
}

/// Where `record` sits in an operator page: the Postgres store's keyset,
/// `(created_at, key, fleet_id)`, compared the same way.
fn position(record: &Record) -> (i64, &str, &Uuid7) {
    (record.created_at_ms, &record.key, &record.fleet)
}

/// Whether `row` is another fleet's shared entry in `owner`'s workspace.
fn shared_with(owner: Owner<'_>, row: &Row) -> bool {
    &row.workspace == owner.workspace
        && !row.record.written_by(owner.fleet)
        && row.record.visibility.is_workspace()
}

/// Whether `text` holds `query`, ignoring ASCII case.
fn holds(text: &str, query: &str) -> bool {
    text.to_ascii_lowercase()
        .contains(&query.to_ascii_lowercase())
}

#[async_trait::async_trait]
impl MemoryStore for InMemory {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn window(&self, owner: Owner<'_>, reads: bool) -> Result<Vec<Record>> {
        let mut rows = self.select(|row| row.record.written_by(owner.fleet), newest);
        if reads {
            rows.extend(self.select(|row| shared_with(owner, row), newest));
        }
        Ok(rows)
    }

    async fn upsert(
        &self,
        owner: Owner<'_>,
        entries: &[&MemoryDelta<'_>],
        now: UnixMillis,
    ) -> Result<Housekept> {
        let at = now.as_millis();
        self.rows.rcu(|rows| {
            let mut next = Rows::clone(rows);
            for delta in entries {
                let id = (owner.fleet.clone(), delta.key.to_string());
                let created = next.get(&id).map_or(at, |row| row.record.created_at_ms);
                let record = Record {
                    fleet: owner.fleet.clone(),
                    key: delta.key.to_string(),
                    content: delta.content.to_string(),
                    category: delta.category.to_string(),
                    visibility: delta.visibility,
                    created_at_ms: created,
                    updated_at_ms: at,
                };
                let workspace = owner.workspace.clone();
                next.insert(id, Row { workspace, record });
            }
            next
        });
        Ok(Housekept::default())
    }

    async fn search(
        &self,
        owner: Owner<'_>,
        reads: bool,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Record>> {
        let key_first = |left: &Record, right: &Record| {
            holds(&right.key, query)
                .cmp(&holds(&left.key, query))
                .then_with(|| newest(left, right))
        };
        let found = |record: &Record| holds(&record.key, query) || holds(&record.content, query);
        let mut rows = self.select(
            |row| row.record.written_by(owner.fleet) && found(&row.record),
            key_first,
        );
        rows.truncate(limit);
        if reads {
            let mut shared = self.select(
                |row| shared_with(owner, row) && found(&row.record),
                key_first,
            );
            shared.truncate(limit);
            rows.extend(shared);
        }
        Ok(rows)
    }

    async fn page(
        &self,
        owner: Owner<'_>,
        reads: bool,
        view: View<'_>,
        after: Option<After<'_>>,
        limit: i64,
    ) -> Result<Vec<Record>> {
        let in_view = |record: &Record| match view {
            View::Recent => true,
            View::Category(label) => record.category == label,
            View::Search(text) => holds(&record.key, text) || holds(&record.content, text),
        };
        let past = |record: &Record| {
            after.is_none_or(|boundary| {
                position(record) < (boundary.created_at_ms, boundary.key, boundary.fleet)
            })
        };
        let mut rows = self.select(
            |row| {
                (row.record.written_by(owner.fleet) || (reads && shared_with(owner, row)))
                    && in_view(&row.record)
                    && past(&row.record)
            },
            |left, right| position(right).cmp(&position(left)),
        );
        rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        Ok(rows)
    }

    async fn forget(&self, owner: Owner<'_>, key: &str) -> Result<bool> {
        let id = (owner.fleet.clone(), key.to_owned());
        let previous = self.rows.rcu(|rows| {
            let mut next = Rows::clone(rows);
            next.remove(&id);
            next
        });
        Ok(previous.contains_key(&id))
    }

    async fn forget_stale(&self, owner: Owner<'_>, key: &str, seen_ms: i64) -> Result<bool> {
        let id = (owner.fleet.clone(), key.to_owned());
        let stale = |rows: &Rows| {
            rows.get(&id)
                .is_some_and(|row| row.record.updated_at_ms <= seen_ms)
        };
        let previous = self.rows.rcu(|rows| {
            let mut next = Rows::clone(rows);
            if stale(rows) {
                next.remove(&id);
            }
            next
        });
        Ok(stale(&previous))
    }

    async fn export(&self, workspace: &Uuid7) -> Result<Vec<Record>> {
        Ok(self.select(|row| &row.workspace == workspace, newest))
    }

    async fn import(&self, workspace: &Uuid7, record: &Record) -> Result<()> {
        let id = (record.fleet.clone(), record.key.clone());
        self.rows.rcu(|rows| {
            let newer_here = rows
                .get(&id)
                .is_some_and(|row| row.record.updated_at_ms >= record.updated_at_ms);
            let mut next = Rows::clone(rows);
            if !newer_here {
                let row = Row {
                    workspace: workspace.clone(),
                    record: record.clone(),
                };
                next.insert(id.clone(), row);
            }
            Arc::new(next)
        });
        Ok(())
    }
}

#[cfg(test)]
#[path = "in_memory_tests.rs"]
mod tests;
