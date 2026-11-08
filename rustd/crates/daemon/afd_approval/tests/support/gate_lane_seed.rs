//! Gate seeders for [`Lane`]: the pending rows a test acts on.
//!
//! Split from `gate_lane.rs` (RULE FLL) along the seam `gate_lane_read.rs`
//! already draws: that file builds the lane, this one writes the gates a test
//! answers or sweeps, and the read file checks what happened to them. A second
//! `impl Lane` block, so callers reach these exactly as before —
//! `lane.seed_gate(...)`.

use std::time::Duration;

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

/// The kind a fixture gate carries.
///
/// Deliberately NOT `integration_grant`: that kind makes the resolve write a
/// second table, and these tests are about the gate row's own transition.
const KIND: &str = "repository_write";

impl Lane {
    /// Seeds one pending gate, returning its action id.
    ///
    /// `window` is a parameter because the sweeper tests need a deadline in
    /// the past and the race tests need one that has not arrived — a fixture
    /// that fixed it would make one of the two impossible to write.
    pub(crate) async fn seed_gate(&self, window: Window) -> String {
        let action = afd_db::test_util::mint_id();
        self.seed_gate_for(&action, window).await;
        action
    }

    /// One pending gate that held no run: `event_id` is NULL, the shape of a
    /// standing grant raised at install time rather than by an event.
    pub(crate) async fn seed_runless_gate(&self, window: Window) -> String {
        let action = afd_db::test_util::mint_id();
        self.seed_row(&action, window, NOW, None).await;
        action
    }

    /// One pending gate row for an action that may already have one — a
    /// re-raised action, which the park writes as a second row rather than
    /// reopening the first.
    pub(crate) async fn seed_gate_for(&self, action: &str, window: Window) {
        self.seed_row(action, window, NOW, Some(afd_db::test_util::mint_id()))
            .await;
    }

    /// Seeds one pending gate raised at `created_at`, returning its action id.
    ///
    /// Every other seeder stamps [`NOW`], which is what the ordering and keyset
    /// tests need control of: one proves the page walks newest-first and
    /// therefore needs distinct instants, the other proves the cursor's TUPLE
    /// comparison and therefore needs several rows sharing one.
    pub(crate) async fn seed_gate_at(&self, created_at: Timestamp, window: Window) -> String {
        let action = afd_db::test_util::mint_id();
        self.seed_row(
            &action,
            window,
            created_at,
            Some(afd_db::test_util::mint_id()),
        )
        .await;
        action
    }

    /// Inserts one pending gate row: what every seeder above writes through.
    ///
    /// The deadline is `now()` plus or minus [`WINDOW`], on the clock the
    /// sweeper reads; the raised-at instant is the caller's, because only the
    /// page and cursor tests read it and they need to choose it.
    async fn seed_row(
        &self,
        action: &str,
        window: Window,
        created_at: Timestamp,
        event_id: Option<String>,
    ) {
        sqlx::query(
            "INSERT INTO fleet.approval_gates
               (id, fleet_id, workspace_id, action_id, tool_name, action_name,
                gate_type, proposed_action, evidence, blast_radius, expires_at,
                status, detail, created_at, updated_at, event_id,
                spend_count, spend_ceiling)
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'git', 'push',
                     $5, 'open a pull request', '{}'::jsonb, 'one repository',
                     CASE WHEN $6 THEN now() - $7::interval ELSE now() + $7::interval END,
                     $8, '', $9, $9, $10, 0, 32)",
        )
        .bind(mint().as_str())
        .bind(self.fleet.as_str())
        .bind(self.workspace.as_str())
        .bind(action)
        .bind(KIND)
        .bind(window == Window::Lapsed)
        .bind(WINDOW)
        .bind(afd_api_wire::approval::status::PENDING)
        .bind(created_at.to_sqlx())
        .bind(event_id)
        .execute(&mut *self.connection().await)
        .await
        .expect("the gate row must insert");
    }
}
