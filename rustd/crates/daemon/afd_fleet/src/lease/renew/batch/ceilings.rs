//! Each fleet's budget ceilings, read once for a frame and cached by revision.

use std::collections::HashMap;

use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use afd_billing::Nanos;

use super::rows::Found;
use crate::error::{Result, query};
use crate::lease::cache::RevisionKey;
use crate::lease::coverage::admit_unreadable_fleet;
use crate::lease::pull::LeaseService;
use crate::lease::sql;

/// What query failures name the revision read as.
const CONTEXT_REVISIONS: &str = "renewal batch revisions";

/// The gate a fleet whose ceiling cannot be read is admitted past.
const GATE: &str = "budget";

/// A fleet's ceilings, as its head revision states them.
#[derive(Debug, Clone, Copy)]
pub(super) struct Ceilings {
    pub(super) daily: i64,
    pub(super) monthly: Option<i64>,
}

/// A fleet's ceilings as the batch could read them.
pub(super) enum Read {
    /// The head revision's ceilings.
    Ceilings(Ceilings),
    /// The stored budget will not parse: a ceiling this daemon cannot read is
    /// not one it may ignore, so the fleet's leases are refused, with the fault
    /// that says why.
    Unreadable(crate::Error),
    /// The revision could not be read at all: admitted, as the per-lease
    /// path admits an outage, and checked again next tick.
    Unread,
}

impl LeaseService {
    /// Each fleet's ceilings, from the head revision's definition: cached by
    /// revision id, with one read for every revision the cache misses.
    pub(super) async fn ceilings(&self, batch: &[(&Found, &RenewRequest)]) -> HashMap<Uuid7, Read> {
        let mut read = HashMap::new();
        let mut missing: Vec<(&Uuid7, &Uuid7)> = Vec::new();
        for (row, _request) in batch {
            if read.contains_key(&row.fleet_id) {
                continue;
            }
            let Some(revision) = row.head_revision_id.as_ref() else {
                read.insert(row.fleet_id.clone(), Read::Unread);
                continue;
            };
            let key = RevisionKey::observed(revision.clone());
            if let Some(definition) = self.leases.definitions().get(&key).await {
                read.insert(
                    row.fleet_id.clone(),
                    Read::Ceilings(ceilings_of(&definition)),
                );
            } else {
                read.insert(row.fleet_id.clone(), Read::Unread);
                missing.push((&row.fleet_id, revision));
            }
        }
        if !missing.is_empty() {
            self.parse_missing(&missing, &mut read).await;
        }
        read
    }

    /// Reads the revisions the cache missed in one statement and parses each,
    /// leaving a fleet `Unread` when the read itself failed.
    async fn parse_missing(&self, missing: &[(&Uuid7, &Uuid7)], read: &mut HashMap<Uuid7, Read>) {
        let revisions: Vec<&Uuid7> = missing.iter().map(|(_fleet, revision)| *revision).collect();
        let documents: Vec<(Uuid7, String)> = match self.revision_documents(&revisions).await {
            Ok(documents) => documents,
            // Admitted, as an outage reading the ceiling is on the per-lease path,
            // and said once per fleet through the same event.
            Err(outage) => {
                for (fleet, _revision) in missing {
                    admit_unreadable_fleet(GATE, fleet, &outage);
                }
                return;
            }
        };
        let documents: HashMap<&Uuid7, &str> = documents
            .iter()
            .map(|(id, text)| (id, text.as_str()))
            .collect();
        for (fleet, revision) in missing {
            let Some(document) = documents.get(revision) else {
                continue;
            };
            let key = RevisionKey::observed((*revision).clone());
            let parsed = self
                .leases
                .definitions()
                .get_or_parse(&key, document, crate::lease::installed::parse_for_save)
                .await;
            let outcome = match parsed {
                Ok(definition) => Read::Ceilings(ceilings_of(&definition)),
                Err(fault) if fault.is_config_permanent() => Read::Unreadable(fault),
                Err(outage) => {
                    admit_unreadable_fleet(GATE, fleet, &outage);
                    Read::Unread
                }
            };
            read.insert((*fleet).clone(), outcome);
        }
    }

    async fn revision_documents(&self, revisions: &[&Uuid7]) -> Result<Vec<(Uuid7, String)>> {
        sqlx::query_as(sql::renew::SELECT_REVISION_DOCUMENTS)
            .bind(revisions)
            .fetch_all(&mut *self.leases.pool().acquire().await?)
            .await
            .map_err(query(CONTEXT_REVISIONS))
    }
}

/// A definition's ceilings in nanos, as `budget::covers` reads them.
fn ceilings_of(definition: &af_fleet_runtime::Definition) -> Ceilings {
    let budget = definition.runtime().budget();
    Ceilings {
        daily: Nanos::from_ceiling(budget.daily()).as_i64(),
        monthly: budget
            .monthly()
            .map(|monthly| Nanos::from_ceiling(monthly).as_i64()),
    }
}
