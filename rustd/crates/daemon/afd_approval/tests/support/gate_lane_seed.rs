//! Gate seeders for [`Lane`]: the pending rows a test acts on.
//!
//! Split from `gate_lane.rs` (RULE FLL) along the seam `gate_lane_read.rs`
//! already draws: that file builds the lane, this one writes the gates a test
//! answers or sweeps, and the read file checks what happened to them. A second
//! `impl Lane` block, so callers reach these exactly as before —
//! `lane.seed_gate(...)`.

use af_core::id::Uuid7;
use std::time::Duration;

use afd_api_wire::approval::GATE_TYPE_QUESTION;
use afd_approval::KIND_INTEGRATION_GRANT;
use afd_db::test_util::{SEEDED_TURN, TurnSeed};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use super::{Lane, NOW, mint};

/// How far from the database's clock a fixture gate's deadline sits.
const WINDOW: Duration = Duration::from_hours(1);

/// Where a fixture gate's deadline sits against the database's clock.
///
/// The sweeper reads `now()`, so a deadline written as a fixed instant would be
/// in the past or the future depending on the day the suite runs. Each seeder
/// places it relative to the clock that will judge it instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Window {
    /// The deadline is ahead: the gate waits, and a sweep leaves it.
    Open,
    /// The deadline has passed: a sweep takes the gate.
    Lapsed,
}

/// Fleet delete's first hold (`afd_setup/src/creation/sql/lock.sql`).
pub(crate) const LOCK_FLEET: &str =
    "SELECT pg_backend_pid() FROM fleet.fleets WHERE id = $1 FOR UPDATE";

/// A queued fleet turn a run is answering, as admission leaves it.
const QUEUED_TURN: &str = "INSERT INTO thread.turns (id,thread_id,workspace_id,producer,producer_key,\
payload_digest,actor,kind,status,request_json,created_at,updated_at) VALUES \
($1,$2,$3,'steer',$1,'fixture','steer:fixture','chat','queued','{}',now(),now())";

impl Lane {
    /// Seeds one pending access card on a turn of its own, returning its
    /// action id.
    ///
    /// The card a person approves or denies, which is the shape most inbox
    /// verbs answer. It names no service, so its answer moves no grant: these
    /// tests are about the gate row and the answer turn, and the grant suites
    /// raise their cards through the request verb instead.
    ///
    /// `window` is a parameter because the late-answer tests need a deadline
    /// in the past and the race tests need one that has not arrived.
    pub(crate) async fn seed_gate(&self, window: Window) -> Uuid7 {
        let action = mint();
        self.seed_row(&action, window, NOW, KIND_INTEGRATION_GRANT)
            .await;
        action
    }

    /// One pending `question` gate on a turn of its own, the shape a run's
    /// `waiting` report raises, returning its action id.
    pub(crate) async fn seed_question(&self, window: Window) -> Uuid7 {
        let action = mint();
        self.seed_question_for(&action, window).await;
        action
    }

    /// One pending question row for an action that may already have one — a
    /// re-raised action, written as a second row rather than reopening the
    /// first.
    pub(crate) async fn seed_question_for(&self, action: &Uuid7, window: Window) {
        self.seed_row(action, window, NOW, GATE_TYPE_QUESTION).await;
    }

    /// Seeds one pending access card raised at `created_at`, returning its
    /// action id.
    ///
    /// Every other seeder stamps [`NOW`], which is what the ordering and keyset
    /// tests need control of: one proves the page walks newest-first and
    /// therefore needs distinct instants, the other proves the cursor's TUPLE
    /// comparison and therefore needs several rows sharing one.
    pub(crate) async fn seed_gate_at(&self, created_at: Timestamp, window: Window) -> Uuid7 {
        let action = mint();
        self.seed_row(&action, window, created_at, KIND_INTEGRATION_GRANT)
            .await;
        action
    }

    /// A queued turn of the lane's fleet, as admission leaves it.
    pub(crate) async fn queued_turn(&self) -> Uuid7 {
        let id = mint();
        sqlx::query(QUEUED_TURN)
            .bind(&id)
            .bind(&self.fleet)
            .bind(&self.workspace)
            .execute(&mut *self.connection().await)
            .await
            .expect("the queued turn inserts");
        id
    }

    /// Points `action`'s gate at `turn`.
    pub(crate) async fn gate_turn(&self, action: &Uuid7, turn: Option<&Uuid7>) {
        sqlx::query("UPDATE fleet.approval_gates SET turn_id = $2 WHERE action_id = $1")
            .bind(action)
            .bind(turn)
            .execute(&mut *self.connection().await)
            .await
            .expect("the gate moves");
    }

    /// Moves `action`'s deadline to `ago` before the database's clock.
    pub(crate) async fn lapse_by(&self, action: &Uuid7, ago: Duration) {
        sqlx::query(
            "UPDATE fleet.approval_gates SET expires_at = now() - $2::interval \
             WHERE action_id = $1",
        )
        .bind(action)
        .bind(ago)
        .execute(&mut *self.connection().await)
        .await
        .expect("the deadline moves");
    }

    /// Archives the lane's fleet.
    pub(crate) async fn archive_fleet(&self) {
        sqlx::query("UPDATE fleet.fleets SET status = $1 WHERE id = $2")
            .bind(afd_fleet_lifecycle::FleetStatus::Archived.as_str())
            .bind(&self.fleet)
            .execute(&mut *self.connection().await)
            .await
            .expect("the fleet archives");
    }

    /// Seeds the turn a gate keeps waiting: its answer turn resumes it, and
    /// `fk_turns_resumes_turn_id` refuses an answer to no turn.
    pub(crate) async fn seed_turn(&self, turn: &Uuid7) {
        TurnSeed::new(&self.fleet, &self.workspace, turn, SEEDED_TURN)
            .insert(&mut *self.connection().await)
            .await
            .expect("the gated turn must insert");
    }

    /// A fresh turn of the lane's fleet for a run to ask on.
    pub(crate) async fn asking_turn(&self) -> Uuid7 {
        let turn = mint();
        self.seed_turn(&turn).await;
        turn
    }

    /// Inserts one pending gate row of `gate_type` on a fresh turn: what every
    /// seeder above writes through.
    ///
    /// The deadline is `now()` plus or minus [`WINDOW`], on the clock the
    /// sweeper reads; the raised-at instant is the caller's, because only the
    /// page and cursor tests read it and they need to choose it.
    async fn seed_row(
        &self,
        action: &Uuid7,
        window: Window,
        created_at: Timestamp,
        gate_type: &str,
    ) {
        let turn = self.asking_turn().await;
        sqlx::query(
            "INSERT INTO fleet.approval_gates
               (id, fleet_id, workspace_id, action_id, tool_name, action_name,
                gate_type, proposed_action, evidence, blast_radius, expires_at,
                status, detail, created_at, updated_at, turn_id)
             VALUES ($1, $2, $3, $4, 'fixture', 'fixture',
                     $5, 'use the workspace secret fixture', '{}'::jsonb, '',
                     CASE WHEN $6 THEN now() - $7::interval ELSE now() + $7::interval END,
                     $8, '', $9, $9, $10)",
        )
        .bind(mint())
        .bind(&self.fleet)
        .bind(&self.workspace)
        .bind(action)
        .bind(gate_type)
        .bind(window == Window::Lapsed)
        .bind(WINDOW)
        .bind(afd_api_wire::approval::status::PENDING)
        .bind(created_at.to_sqlx())
        .bind(turn)
        .execute(&mut *self.connection().await)
        .await
        .expect("the gate row must insert");
    }
}
