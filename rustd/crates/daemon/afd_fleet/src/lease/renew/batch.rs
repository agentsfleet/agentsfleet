//! One host's renewals in one statement (M245 §7).
//!
//! The runner sends one renewal frame per host each tick, naming every live
//! lease. Before, each lease ran the per-lease path: a load, a funds check, a
//! budget read, a price read, the host lock, a cancel read, the meter and a
//! line log, about ten statements. Here the frame costs a pre-read and one
//! statement ([`sql::renew::RENEW_BATCH`]), plus a funds check per payer every
//! [`FUNDS_CHECK_TICKS`] ticks and a read for whatever the caches miss.
//!
//! A lease that runs a durable task keeps the per-lease path, which checks task
//! authority at commit (blocker 3). So does a lease whose prices or ceilings
//! moved twice inside one tick: the per-lease path reads them fresh, and the
//! case is rare enough that its statements do not count.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use af_wire::usage::{LineKind, RUNTIME_MS_MAX};
use afd_billing::Nanos;
use afd_billing::price::RatedLine;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use crate::error::{
    Result, budget_exhausted, lease_cancelled, lease_lost, lease_max_runtime, lease_not_found,
    query, renewal_no_credits,
};
use crate::lease::cache::RevisionKey;
use crate::lease::pull::LeaseService;
use crate::lease::renew::Renewing;
use crate::lease::sql;
use crate::lease::usage::Pricing;

/// How many renewal ticks pass between funds checks of one payer.
///
/// A two-tick chat then pays two thirds of a check instead of four statements,
/// and a run overruns its payer's balance by at most this many slices, the
/// overrun Indy accepted (M245 §7).
pub const FUNDS_CHECK_TICKS: u64 = 3;

/// Whether a session's renewal frame `tick`, counted from 0, checks funds.
///
/// The first frame checks, then every [`FUNDS_CHECK_TICKS`]th, so a reconnect
/// checks at once and a runner cannot dodge the check by reconnecting.
#[must_use]
pub const fn funds_due(tick: u64) -> bool {
    tick.is_multiple_of(FUNDS_CHECK_TICKS)
}

/// What query failures name the batch's reads and statement as.
const CONTEXT_READ: &str = "renewal batch read";
const CONTEXT_REVISIONS: &str = "renewal batch revisions";
const CONTEXT_RENEW: &str = "renewal batch";

/// Logged when a lease's prices or ceilings moved under its statement.
const EVENT_REPRICED: &str = "renew_batch_repriced";

/// Logged when the frame's pre-read or statement failed.
const EVENT_FRAME_FAILED: &str = "renew_batch_failed";

/// One host's renewal frame, answered lease by lease in its order.
#[derive(Debug)]
pub struct Renewals<'a> {
    /// The host that sent the frame.
    pub runner_id: &'a Uuid7,
    /// Every lease the frame names, with its request lines.
    pub leases: Vec<(Uuid7, RenewRequest)>,
    /// Whether this tick checks each payer's funds.
    pub funds_due: bool,
    /// The instant identifiers this pass mints are drawn at.
    pub now: Timestamp,
}

/// One lease as the pre-read found it.
#[derive(Debug, sqlx::FromRow)]
struct Found {
    id: Uuid7,
    status: String,
    cancelled: bool,
    tenant_id: Uuid7,
    workspace_id: Uuid7,
    fleet_id: Uuid7,
    posture: String,
    provider: String,
    model: String,
    task_id: Option<Uuid7>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    head_revision_id: Option<Uuid7>,
    catalogue_version: i64,
}

impl Found {
    fn renewing(&self) -> Renewing {
        Renewing {
            cancelled: self.cancelled,
            tenant_id: self.tenant_id.clone(),
            fleet_id: self.fleet_id.clone(),
            workspace_id: self.workspace_id.clone(),
            posture: self.posture.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            status: self.status.clone(),
        }
    }
}

/// A fleet's ceilings, as its head revision states them.
#[derive(Debug, Clone, Copy)]
struct Ceilings {
    daily: i64,
    monthly: Option<i64>,
}

/// A lease bound for the statement, with what it was priced and judged under.
struct Bound<'a> {
    found: &'a Found,
    request: &'a RenewRequest,
    pricing: Arc<Pricing>,
    ceilings: Option<Ceilings>,
}

/// One lease's row of the batch statement.
#[derive(Debug, sqlx::FromRow)]
struct Judged {
    lease_id: Uuid7,
    found: bool,
    cancelled: Option<bool>,
    fence_holds: Option<bool>,
    below_cap: Option<bool>,
    priced_as_held: Option<bool>,
    ceilings_current: Option<bool>,
    over_budget: Option<bool>,
    host_renews: Option<bool>,
    admitted: Option<bool>,
    extended_until: Option<jiff_sqlx::Timestamp>,
    slot_moved: bool,
    charged_nanos: i64,
}

/// What one lease's row decided.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Extended(Timestamp, Nanos),
    Lost,
    Cancelled,
    MaxRuntime,
    OverBudget,
    /// Its prices or ceilings moved under the statement: priced again and run
    /// once more, then the per-lease path.
    Moved,
}

impl Judged {
    /// The order of these tests is the per-lease path's: a lease that is not
    /// this runner's live holder is lost before anything else is asked of it.
    fn verdict(&self) -> Verdict {
        let held = |check: Option<bool>| check.unwrap_or(false);
        if !self.found || !held(self.host_renews) || !held(self.admitted) {
            return Verdict::Lost;
        }
        if held(self.cancelled) {
            return Verdict::Cancelled;
        }
        if !held(self.fence_holds) {
            return Verdict::Lost;
        }
        if !held(self.below_cap) {
            return Verdict::MaxRuntime;
        }
        if !held(self.priced_as_held) || !held(self.ceilings_current) {
            return Verdict::Moved;
        }
        if held(self.over_budget) {
            return Verdict::OverBudget;
        }
        match self.extended_until {
            // Both rows moved, or the lease is lost: a deadline the slot does
            // not honour would run the child past a reclaim.
            Some(until) if self.slot_moved => {
                Verdict::Extended(until.to_jiff(), Nanos::from_i64(self.charged_nanos))
            }
            _ => Verdict::Lost,
        }
    }
}

impl LeaseService {
    /// Renews every lease one host's frame names, and answers each in order.
    ///
    /// # Errors
    /// Each answer is a lease's own: the refusals [`LeaseService::renew`]
    /// answers, or a fault reaching the datastore, which the runner retries at
    /// its next tick.
    pub async fn renew_batch(&self, renewals: Renewals<'_>) -> Vec<Result<(Timestamp, Nanos)>> {
        let Renewals {
            runner_id,
            leases,
            funds_due,
            now,
        } = renewals;
        // A frame the batch could not read or run leaves every lease to the
        // per-lease path: in an outage each one faults on its own and answers
        // retry, and the runner keeps them until their deadlines (7.5).
        let mut answers = match self.renew_frame(runner_id, &leases, funds_due, now).await {
            Ok(answers) => answers,
            Err(fault) => {
                tracing::warn!(
                    runner_id = runner_id.as_str(),
                    error_code = fault.code().as_str(),
                    event = EVENT_FRAME_FAILED,
                    "the renewal statement did not run; each lease renews on its own path"
                );
                HashMap::new()
            }
        };
        let mut ordered = Vec::with_capacity(leases.len());
        for (lease_id, request) in leases {
            let answer = match answers.remove(&lease_id) {
                Some(Answer::Done(answer)) => answer,
                Some(Answer::PerLease) | None => {
                    self.renew(runner_id, &lease_id, request, now).await
                }
            };
            ordered.push(answer);
        }
        ordered
    }

    /// The frame's answers by lease, or a fault before any lease was judged.
    async fn renew_frame(
        &self,
        runner_id: &Uuid7,
        leases: &[(Uuid7, RenewRequest)],
        funds_due: bool,
        now: Timestamp,
    ) -> Result<HashMap<Uuid7, Answer>> {
        let ids: Vec<&Uuid7> = leases.iter().map(|(id, _request)| id).collect();
        let found: Vec<Found> = sqlx::query_as(sql::renew::SELECT_RENEWALS)
            .bind(&ids)
            .bind(runner_id)
            .fetch_all(&mut *self.leases.pool().acquire().await?)
            .await
            .map_err(query(CONTEXT_READ))?;
        let found: HashMap<&Uuid7, &Found> = found.iter().map(|row| (&row.id, row)).collect();

        let mut answers = HashMap::new();
        let mut batch = Vec::new();
        for (lease_id, request) in leases {
            let Some(row) = found.get(lease_id) else {
                answers.insert(lease_id.clone(), Answer::Done(Err(lease_not_found())));
                continue;
            };
            if row.task_id.is_some() {
                answers.insert(lease_id.clone(), Answer::PerLease);
                continue;
            }
            batch.push((*row, request));
        }

        let refused = self.refusals(runner_id, &batch, funds_due, now).await;
        let ceilings = self.ceilings(&batch).await;
        let mut bound = Vec::new();
        for (row, request) in batch {
            if refused.contains(&(&row.tenant_id, &row.workspace_id)) {
                self.known_lines(runner_id, row, request, now).await;
                answers.insert(row.id.clone(), Answer::Done(Err(renewal_no_credits())));
                continue;
            }
            let ceilings = match ceilings.get(&row.fleet_id) {
                Some(Read::Unreadable) => {
                    self.known_lines(runner_id, row, request, now).await;
                    answers.insert(row.id.clone(), Answer::Done(Err(budget_exhausted())));
                    continue;
                }
                Some(Read::Ceilings(ceilings)) => Some(*ceilings),
                Some(Read::Unread) | None => None,
            };
            let pricing = self.pricing_of(row).await;
            bound.push(Bound {
                found: row,
                request,
                pricing,
                ceilings,
            });
        }

        let judged = self.run_batch(runner_id, &bound, now).await?;
        let mut moved = Vec::new();
        for (entry, verdict) in bound.into_iter().zip(judged) {
            let lease_id = entry.found.id.clone();
            let answer = match verdict {
                Verdict::Extended(until, charged) => Ok((until, charged)),
                Verdict::Moved => {
                    moved.push(entry);
                    continue;
                }
                refused => {
                    self.known_lines(runner_id, entry.found, entry.request, now)
                        .await;
                    Err(refusal_of(&refused))
                }
            };
            answers.insert(lease_id, Answer::Done(answer));
        }
        if !moved.is_empty() {
            tracing::info!(
                runner_id = runner_id.as_str(),
                leases = moved.len(),
                event = EVENT_REPRICED,
                "prices or ceilings moved under the renewal statement; the leases take the per-lease path"
            );
            for entry in moved {
                answers.insert(entry.found.id.clone(), Answer::PerLease);
            }
        }
        Ok(answers)
    }
}

/// A lease's answer, or the per-lease path's to give.
enum Answer {
    Done(Result<(Timestamp, Nanos)>),
    PerLease,
}

/// The refusal a lease's verdict answers with.
fn refusal_of(verdict: &Verdict) -> crate::Error {
    match verdict {
        Verdict::Cancelled => lease_cancelled(),
        Verdict::MaxRuntime => lease_max_runtime(),
        Verdict::OverBudget => budget_exhausted(),
        Verdict::Lost | Verdict::Extended(..) | Verdict::Moved => lease_lost(),
    }
}

/// A fleet's ceilings as the batch could read them.
enum Read {
    /// The head revision's ceilings.
    Ceilings(Ceilings),
    /// The stored budget will not parse: a ceiling this daemon cannot read is
    /// not one it may ignore, so the fleet's leases are refused.
    Unreadable,
    /// The revision could not be read at all: admitted, as the per-lease
    /// path admits an outage, and checked again next tick.
    Unread,
}

impl LeaseService {
    /// The payers this tick's funds check found short, checked once each.
    ///
    /// Only every [`FUNDS_CHECK_TICKS`]th tick checks: the cadence is the
    /// gateway session's, so a reconnect checks on its first tick.
    async fn refusals<'a>(
        &self,
        _runner_id: &Uuid7,
        batch: &[(&'a Found, &RenewRequest)],
        funds_due: bool,
        now: Timestamp,
    ) -> HashSet<(&'a Uuid7, &'a Uuid7)> {
        let mut short = HashSet::new();
        if !funds_due {
            return short;
        }
        let mut checked = HashSet::new();
        for (row, _request) in batch {
            let payer = (&row.tenant_id, &row.workspace_id);
            if !checked.insert(payer) {
                continue;
            }
            let renewing = row.renewing();
            let estimate = self.cached_floor(&renewing, row, now).await;
            if self
                .funds_cover_at(&renewing, &row.id, row.id.as_str(), estimate)
                .await
                .is_err()
            {
                short.insert(payer);
            }
        }
        short
    }

    /// Each fleet's ceilings, from the head revision's definition: cached by
    /// revision id, with one read for every revision the cache misses.
    async fn ceilings(&self, batch: &[(&Found, &RenewRequest)]) -> HashMap<Uuid7, Read> {
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
        if missing.is_empty() {
            return read;
        }
        let revisions: Vec<&Uuid7> = missing.iter().map(|(_fleet, revision)| *revision).collect();
        let documents: Vec<(Uuid7, String)> = match self.revision_documents(&revisions).await {
            Ok(documents) => documents,
            // Admitted, as an outage reading the ceiling is on the per-lease path.
            Err(_outage) => return read,
        };
        let documents: HashMap<&Uuid7, &str> = documents
            .iter()
            .map(|(id, text)| (id, text.as_str()))
            .collect();
        for (fleet, revision) in missing {
            let Some(document) = documents.get(revision) else {
                continue;
            };
            let key = RevisionKey::observed(revision.clone());
            let parsed = self
                .leases
                .definitions()
                .get_or_parse(&key, document, crate::lease::installed::parse_for_save)
                .await;
            let outcome = match parsed {
                Ok(definition) => Read::Ceilings(ceilings_of(&definition)),
                Err(fault) if fault.is_config_permanent() => Read::Unreadable,
                Err(_outage) => Read::Unread,
            };
            read.insert(fleet.clone(), outcome);
        }
        read
    }

    async fn revision_documents(&self, revisions: &[&Uuid7]) -> Result<Vec<(Uuid7, String)>> {
        sqlx::query_as(sql::renew::SELECT_REVISION_DOCUMENTS)
            .bind(revisions)
            .fetch_all(&mut *self.leases.pool().acquire().await?)
            .await
            .map_err(query(CONTEXT_REVISIONS))
    }

    /// The funds check's floor, priced from the book cached at the row's
    /// catalogue version: a warm replica reads no price (M245 §9).
    async fn cached_floor(
        &self,
        renewing: &Renewing,
        row: &Found,
        now: Timestamp,
    ) -> Result<afd_billing::rates::Estimate> {
        let posture = crate::lease::renew::posture_of(renewing);
        let pricing = self
            .leases
            .prices()
            .try_get_or_load(
                self.leases.pool(),
                posture,
                &row.provider,
                &row.model,
                Some(row.catalogue_version),
            )
            .await?;
        Ok(afd_billing::Accounts::estimate_from(
            pricing.book(),
            posture,
            now,
        )?)
    }

    /// The lease's pricing, from the price books by catalogue version.
    async fn pricing_of(&self, row: &Found) -> Arc<Pricing> {
        self.leases
            .prices()
            .get_or_load(
                self.leases.pool(),
                Pricing::posture_of(&row.posture),
                &row.provider,
                &row.model,
                Some(row.catalogue_version),
                crate::lease::cache::LABEL_BATCH,
            )
            .await
    }

    /// Logs a refused lease's known request lines, as the per-lease path does
    /// (blocker 4). A failure is the line's to retry with the next frame.
    async fn known_lines(
        &self,
        runner_id: &Uuid7,
        row: &Found,
        request: &RenewRequest,
        now: Timestamp,
    ) {
        if request.requests.0.is_empty() {
            return;
        }
        let pricing = self.pricing_of(row).await;
        if let Err(fault) = self
            .leases
            .record_known_requests(runner_id, &row.id, &pricing, &request.requests.0, now)
            .await
        {
            tracing::warn!(
                lease_id = row.id.as_str(),
                error_code = fault.code().as_str(),
                event = EVENT_FRAME_FAILED,
                "a refused lease's known request lines were not logged"
            );
        }
    }

    /// The batch statement over every bound lease, a verdict for each in order.
    async fn run_batch(
        &self,
        runner_id: &Uuid7,
        bound: &[Bound<'_>],
        now: Timestamp,
    ) -> Result<Vec<Verdict>> {
        if bound.is_empty() {
            return Ok(Vec::new());
        }
        let columns = Columns::of(bound, self.leases.entropy(), now)?;
        let (digest, connection) = self.leases.host_proof(runner_id);
        let rows: Vec<Judged> = columns
            .bind(runner_id)
            .bind(digest)
            .bind(connection)
            .fetch_all(&mut *self.leases.pool().acquire().await?)
            .await
            .map_err(query(CONTEXT_RENEW))?;
        let mut by_lease: HashMap<Uuid7, Judged> = rows
            .into_iter()
            .map(|row| (row.lease_id.clone(), row))
            .collect();
        Ok(bound
            .iter()
            .map(|entry| {
                by_lease
                    .remove(&entry.found.id)
                    .map_or(Verdict::Lost, |row| row.verdict())
            })
            .collect())
    }
}

/// The statement's parallel arrays: one entry per lease, one per request line.
struct Columns<'a> {
    ids: Vec<&'a Uuid7>,
    posture: Vec<&'static str>,
    provider: Vec<&'a str>,
    model: Vec<&'a str>,
    catalogue: Vec<Option<i64>>,
    runtime_price: Vec<Option<Uuid7>>,
    runtime_line: Vec<Uuid7>,
    daily: Vec<Option<i64>>,
    monthly: Vec<Option<i64>>,
    revision: Vec<Option<&'a Uuid7>>,
    lines: Vec<(&'a Uuid7, Uuid7, RatedLine)>,
}

impl<'a> Columns<'a> {
    /// Prices each lease's request lines and mints every id the statement writes.
    fn of(
        bound: &'a [Bound<'a>],
        entropy: &af_crypto::entropy::Entropy,
        now: Timestamp,
    ) -> Result<Self> {
        let mut columns = Self {
            ids: Vec::new(),
            posture: Vec::new(),
            provider: Vec::new(),
            model: Vec::new(),
            catalogue: Vec::new(),
            runtime_price: Vec::new(),
            runtime_line: Vec::new(),
            daily: Vec::new(),
            monthly: Vec::new(),
            revision: Vec::new(),
            lines: Vec::new(),
        };
        for entry in bound {
            let row = entry.found;
            columns.ids.push(&row.id);
            columns.posture.push(entry.pricing.posture().as_str());
            columns.provider.push(entry.pricing.provider());
            columns.model.push(entry.pricing.model());
            columns.catalogue.push(entry.pricing.catalogue_version());
            columns.runtime_price.push(entry.pricing.runtime_price(now));
            columns.runtime_line.push(entropy.uuid7(now)?);
            columns
                .daily
                .push(entry.ceilings.map(|ceilings| ceilings.daily));
            columns
                .monthly
                .push(entry.ceilings.and_then(|ceilings| ceilings.monthly));
            columns.revision.push(row.head_revision_id.as_ref());
            for line in
                entry
                    .pricing
                    .request_lines(row.created_at, now, &entry.request.requests.0)?
            {
                columns.lines.push((&row.id, entropy.uuid7(now)?, line));
            }
        }
        Ok(columns)
    }

    fn line_column(&self, field: fn(&RatedLine) -> i64) -> Vec<i64> {
        self.lines
            .iter()
            .map(|(_lease, _id, line)| field(line))
            .collect()
    }

    /// [`sql::renew::RENEW_BATCH`] with every parameter bound, in `$n` order.
    fn bind(
        &self,
        runner_id: &'a Uuid7,
    ) -> sqlx::query::QueryAs<'a, sqlx::Postgres, Judged, sqlx::postgres::PgArguments> {
        sqlx::query_as(sql::renew::RENEW_BATCH)
            .bind(runner_id)
            .bind(self.ids.clone())
            .bind(self.posture.clone())
            .bind(self.provider.clone())
            .bind(self.model.clone())
            .bind(self.catalogue.clone())
            .bind(self.runtime_price.clone())
            .bind(self.runtime_line.clone())
            .bind(self.daily.clone())
            .bind(self.monthly.clone())
            .bind(self.revision.clone())
            .bind(
                self.lines
                    .iter()
                    .map(|(lease, _id, _line)| *lease)
                    .collect::<Vec<_>>(),
            )
            .bind(
                self.lines
                    .iter()
                    .map(|(_lease, id, _line)| id.clone())
                    .collect::<Vec<_>>(),
            )
            .bind(self.line_column(|line| i64::from(line.usage.seq)))
            .bind(
                self.lines
                    .iter()
                    .map(|(_lease, _id, line)| line.usage.at.to_sqlx())
                    .collect::<Vec<_>>(),
            )
            .bind(self.line_column(|line| i64::from(line.usage.input_tokens)))
            .bind(self.line_column(|line| i64::from(line.usage.cached_input_tokens)))
            .bind(self.line_column(|line| i64::from(line.usage.cache_write_tokens)))
            .bind(self.line_column(|line| i64::from(line.usage.cache_write_5m_tokens)))
            .bind(self.line_column(|line| i64::from(line.usage.cache_write_1h_tokens)))
            .bind(self.line_column(|line| i64::from(line.usage.output_tokens)))
            .bind(
                self.lines
                    .iter()
                    .map(|(_lease, _id, line)| line.price.clone())
                    .collect::<Vec<_>>(),
            )
            .bind(self.line_column(|line| i64::try_from(line.unpriced_tokens).unwrap_or(i64::MAX)))
            .bind(self.line_column(|line| line.charged.as_i64()))
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(afd_billing::RUN_NANOS_PER_SEC)
            .bind(af_core::timing::LEASE_TTL)
            .bind(af_core::timing::MAX_RUNTIME)
            .bind(afd_billing::sql::work::FLEET_LEASE)
            .bind(LineKind::Model.as_str())
            .bind(LineKind::Runtime.as_str())
            .bind(i64::from(RUNTIME_MS_MAX))
            .bind(af_core::timing::DAY)
            .bind(afd_runner::capacity::renewable_states())
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

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]
mod tests {
    use af_wire::report::RenewRequest;

    use super::{Judged, Renewals, Verdict, funds_due};
    use crate::lease::test_dead;

    #[test]
    fn funds_are_checked_on_the_first_tick_then_every_third() {
        let due: Vec<bool> = (0..7).map(funds_due).collect();
        assert_eq!(due, [true, false, false, true, false, false, true]);
    }

    /// A row every check passed, extended with both rows moved.
    fn passing() -> Judged {
        Judged {
            lease_id: test_dead::id(1),
            found: true,
            cancelled: Some(false),
            fence_holds: Some(true),
            below_cap: Some(true),
            priced_as_held: Some(true),
            ceilings_current: Some(true),
            over_budget: Some(false),
            host_renews: Some(true),
            admitted: Some(true),
            extended_until: Some(jiff_sqlx::Timestamp::from(test_dead::AT)),
            slot_moved: true,
            charged_nanos: 7,
        }
    }

    #[test]
    fn a_row_every_check_passed_extends_with_its_charge() {
        assert_eq!(
            passing().verdict(),
            Verdict::Extended(test_dead::AT, afd_billing::Nanos::from_i64(7))
        );
    }

    /// One failed check, and the verdict it must answer.
    type Case = (fn(&mut Judged), Verdict);

    #[test]
    fn each_failed_check_answers_as_the_per_lease_path_would() {
        let cases: [Case; 11] = [
            (|row| row.found = false, Verdict::Lost),
            (|row| row.host_renews = Some(false), Verdict::Lost),
            (|row| row.admitted = None, Verdict::Lost),
            (|row| row.cancelled = Some(true), Verdict::Cancelled),
            (|row| row.fence_holds = Some(false), Verdict::Lost),
            (|row| row.below_cap = Some(false), Verdict::MaxRuntime),
            (|row| row.priced_as_held = Some(false), Verdict::Moved),
            (|row| row.ceilings_current = Some(false), Verdict::Moved),
            (|row| row.over_budget = Some(true), Verdict::OverBudget),
            (|row| row.slot_moved = false, Verdict::Lost),
            (|row| row.extended_until = None, Verdict::Lost),
        ];
        for (index, (fail, expected)) in cases.into_iter().enumerate() {
            let mut row = passing();
            fail(&mut row);
            assert_eq!(row.verdict(), expected, "case {index}");
        }
    }

    #[test]
    fn a_cancelled_lease_that_lost_its_fence_is_still_cancelled() {
        let mut row = passing();
        row.cancelled = Some(true);
        row.fence_holds = Some(false);
        assert_eq!(
            row.verdict(),
            Verdict::Cancelled,
            "the Kill mark is the answer a runner acts on"
        );
    }

    // 7.5: a frame the batch cannot read leaves every lease to the per-lease
    // path, so in an outage each answers its own datastore fault, which the
    // runner retries, and none is answered lost.
    #[tokio::test]
    async fn a_frame_the_datastore_refuses_answers_every_lease_retry() {
        let _counted = test_dead::CACHE_LOOKUPS.lock().await;
        let leases = vec![
            (test_dead::id(1), RenewRequest::default()),
            (test_dead::id(4), RenewRequest::default()),
        ];
        let answers = test_dead::plane()
            .renew_batch(Renewals {
                runner_id: &test_dead::id(9),
                leases,
                funds_due: true,
                now: test_dead::AT,
            })
            .await;
        assert_eq!(answers.len(), 2);
        for answer in answers {
            let fault = answer.expect_err("no datastore, no renewal");
            assert!(
                fault.is_datastore_unavailable(),
                "retried, never lost: {fault}"
            );
        }
    }
}
