//! The draft assignment: one transaction under the host lock takes the draft, its
//! thread and its run, and the assignment is rendered from what it committed.
//!
//! Lock order is the host row, then the draft's creator and draft (`NOWAIT`,
//! so a busy draft never stalls the walk), then the draft's thread and its
//! earlier run. A draft's report takes the draft and then the thread, and holds
//! no host row, so the two never wait on each other.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_crypto::secret::SecretString;
use af_wire::{
    policy::{ExecutionPolicy, NetworkPolicy},
    run::{
        HISTORY_BYTES_MAX, Owner, RunKind, RunPayload, SetupCandidate, SetupWork,
        TURN_TEXT_BYTES_MAX, Turn,
    },
    setup::FailureReason,
    turn::{TurnEnvelope, TurnKind},
};
use afd_billing::rates::Posture;
use afd_runner::capacity::choice::Ticket;
use afd_setup::setup_run::{SetupAssignment, SetupHint, SetupOwed, SetupTurn, setup_tool};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::Connection as _;

use super::DraftTarget;
use crate::error::{Result, query};
use crate::run::{AssignedRun, RunOwner, RunService, runner_runs, sql};

/// What query failures name the draft assignment as.
const CONTEXT: &str = "assign a draft run";

/// The one tool a setup run offers.
const PROPOSE_SETUP: &str = "propose_setup";

/// A committed draft run, ready to render as its assignment.
///
/// Holds the key the assignment carries, so the payload borrows it rather than
/// copying a secret into an owned string.
#[derive(Debug)]
pub struct DraftAssignment {
    run_id: Uuid7,
    fence: i64,
    expires_at: Timestamp,
    turn: SetupTurn,
    api_key: SecretString,
    inference_host: Box<str>,
    tool: serde_json::Value,
}

/// [`sql::shared::ASSIGN_THREAD`]'s winning row, by column name.
#[derive(sqlx::FromRow)]
struct Won {
    fencing_token: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    run_expires_at: Timestamp,
}

/// [`sql::shared::EXPIRE_PRIOR_RUN`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Prior {
    predecessor_run_id: Option<Uuid7>,
    expired_runner_id: Option<Uuid7>,
    earlier_runs: i64,
}

/// What taking a draft's thread came to.
struct Taken {
    /// The thread and whether the assignment reassigns the turn; `None` when
    /// the draft yields nothing now.
    won: Option<(Won, bool)>,
    /// The lapsed holder whose run the assignment expired, counted once the
    /// transaction commits.
    expired: Option<Uuid7>,
}

/// What one candidate's assignment came to.
enum Placement {
    Issued(Box<DraftAssignment>),
    /// The host can take no more work this pass.
    Busy,
    /// This draft yields nothing now; the next may.
    Skipped,
}

impl RunService {
    /// Assigns the oldest waiting draft turn `runner` may run, under the
    /// pass's placement `ticket`, and commits its run.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a key that would not open.
    pub async fn assign_draft(
        &self,
        drafts: &afd_setup::Drafts,
        runner: &Uuid7,
        ticket: Ticket,
        now: Timestamp,
    ) -> Result<Option<DraftAssignment>> {
        for hint in drafts.setup_candidates(runner).await? {
            match self.assign_one(drafts, runner, &hint, ticket, now).await? {
                Placement::Issued(assignment) => return Ok(Some(*assignment)),
                Placement::Busy => return Ok(None),
                Placement::Skipped => {}
            }
        }
        Ok(None)
    }

    async fn assign_one(
        &self,
        drafts: &afd_setup::Drafts,
        runner: &Uuid7,
        hint: &SetupHint,
        ticket: Ticket,
        now: Timestamp,
    ) -> Result<Placement> {
        let mut connection = self.runs.pool().acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let Some(host) = self
            .runs
            .lock_host(&mut tx, runner, &hint.tenant_id)
            .await?
        else {
            return Ok(Placement::Busy);
        };
        if !host.can_assign() || !ticket.permits_revision(host.run_version(), RunKind::Setup) {
            return Ok(Placement::Busy);
        }
        // Before any write: a foreign runner leaves no run and no failure behind.
        if !host.serves() {
            afd_runner::capacity::refused_placement(
                runner,
                afd_runner::capacity::PlacementRefusal::ForeignTenant,
                RunKind::Setup.as_str(),
            );
            return Ok(Placement::Skipped);
        }
        let mut owed = SetupOwed::default();
        let turn = match drafts
            .assign_setup_turn(&mut tx, hint, host.tenant_owned(), &mut owed)
            .await?
        {
            SetupAssignment::Assigned(turn) => *turn,
            SetupAssignment::Passed | SetupAssignment::Busy => {
                tx.commit().await.map_err(query(CONTEXT))?;
                owed.settle(drafts);
                return Ok(Placement::Skipped);
            }
        };
        // The key the runner calls the model with, opened in this transaction
        // under the same locks that pinned the model, before any run exists.
        let Some(route) = self
            .providers
            .registered_route(&mut tx, &turn.tenant_id, &turn.model)
            .await?
        else {
            drafts
                .fail_setup_turn(&mut tx, &turn, FailureReason::ModelUnavailable, &mut owed)
                .await?;
            tx.commit().await.map_err(query(CONTEXT))?;
            owed.settle(drafts);
            return Ok(Placement::Skipped);
        };
        let owner = RunOwner::Draft(turn.draft_id.clone());
        let taken = self
            .take_thread(&mut tx, drafts, runner, &turn, &mut owed)
            .await?;
        let Some((won, reassignment)) = taken.won else {
            tx.commit().await.map_err(query(CONTEXT))?;
            owed.settle(drafts);
            if let Some(expired) = &taken.expired {
                runner_runs::expired(expired);
            }
            return Ok(Placement::Skipped);
        };
        let issue = Issue {
            runner,
            owner: &owner,
            turn: &turn,
            fence: won.fencing_token,
            held_until: won.run_expires_at,
            run_version: host.run_version(),
            reassignment,
            max_runtime: afd_setup::limits::SETUP_RUNTIME,
        };
        let Some(run_id) = self.issue_draft(&mut tx, issue, now).await? else {
            // The host filled, the fence or the choice moved: nothing commits.
            return Ok(Placement::Busy);
        };
        drafts.start_setup_turn(&mut tx, &turn).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        owed.settle(drafts);
        if let Some(expired) = &taken.expired {
            runner_runs::expired(expired);
        }
        runner_runs::acquired(runner);
        let tool = setup_tool(turn.candidate.is_some());
        Ok(Placement::Issued(Box::new(DraftAssignment {
            run_id,
            fence: won.fencing_token,
            expires_at: won.run_expires_at,
            turn,
            api_key: route.api_key,
            inference_host: route.inference_host,
            tool,
        })))
    }

    /// Wins the draft's thread, bumping its fence, and expires the lapsed run
    /// that ran the turn before: the thread and whether this assignment reassigns the
    /// turn, and whose run it expired. No thread when a live runner still holds it,
    /// or when the turn's runs lapsed [`af_core::limits::EVENT_RUNS_MAX`] times,
    /// which fails it and frees the thread rather than reassigning it forever.
    async fn take_thread(
        &self,
        tx: &mut sqlx::PgConnection,
        drafts: &afd_setup::Drafts,
        runner: &Uuid7,
        turn: &SetupTurn,
        owed: &mut SetupOwed,
    ) -> Result<Taken> {
        let owner = RunOwner::Draft(turn.draft_id.clone());
        let won: Option<Won> = sqlx::query_as(sql::shared::ASSIGN_THREAD)
            .bind(owner.fleet_id())
            .bind(owner.draft_id())
            .bind(runner)
            .bind(self.runs.clock().ttl())
            .bind(self.runs.clock().offline_after())
            .bind(sql::RUNNER_STATUS_ACTIVE)
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let Some(won) = won else {
            return Ok(Taken {
                won: None,
                expired: None,
            });
        };
        let prior: Prior = sqlx::query_as(sql::shared::EXPIRE_PRIOR_RUN)
            .bind(owner.fleet_id())
            .bind(owner.draft_id())
            .bind(&turn.message_id)
            .bind(sql::RUN_STATUS_ACTIVE)
            .bind(sql::RUN_STATUS_EXPIRED)
            .fetch_one(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let expired = prior.expired_runner_id;
        if prior.earlier_runs < i64::from(af_core::limits::EVENT_RUNS_MAX) {
            let reassignment = prior.predecessor_run_id.is_some();
            return Ok(Taken {
                won: Some((won, reassignment)),
                expired,
            });
        }
        // Every earlier run lapsed unreported: the turn is lost, not retried forever.
        drafts
            .fail_setup_turn(&mut *tx, turn, FailureReason::ProviderUsageUnknown, owed)
            .await?;
        sqlx::query(sql::shared::RELEASE_RUNLESS_THREAD)
            .bind(owner.thread_id())
            .bind(won.fencing_token)
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        Ok(Taken { won: None, expired })
    }

    /// Writes the draft's run under its assignment's fence with
    /// [`sql::shared::INSERT_RUN`]: the run's id, or `None` when the host
    /// filled or the fence or the placement choice moved.
    async fn issue_draft(
        &self,
        tx: &mut sqlx::PgConnection,
        issue: Issue<'_>,
        now: Timestamp,
    ) -> Result<Option<Uuid7>> {
        let run_id = self.runs.entropy().uuid7(now)?;
        let audit_id = self.runs.entropy().uuid7(now)?;
        let kind = if issue.reassignment {
            "reassigned"
        } else {
            "fresh"
        };
        let metadata = std::collections::BTreeMap::from([
            (afd_runner::sql::meta::RUN_ID, run_id.as_str()),
            (
                afd_runner::sql::meta::TURN_ID,
                issue.turn.message_id.as_str(),
            ),
            (afd_runner::sql::meta::KIND, kind),
        ]);
        let posture = if issue.turn.model.platform_managed {
            Posture::Platform
        } else {
            Posture::SelfManaged
        };
        let created: Option<jiff_sqlx::Timestamp> = sqlx::query_scalar(sql::shared::INSERT_RUN)
            .bind(&run_id)
            .bind(issue.runner)
            .bind(issue.owner.fleet_id())
            .bind(issue.owner.draft_id())
            .bind(&issue.turn.workspace_id)
            .bind(&issue.turn.tenant_id)
            .bind(&issue.turn.message_id)
            .bind(issue.turn.message_created_at.to_sqlx())
            .bind(posture.as_str())
            .bind(issue.turn.model.provider.as_ref())
            .bind(issue.turn.model.model.as_ref())
            .bind(issue.fence)
            .bind(sql::RUN_STATUS_ACTIVE)
            .bind(&audit_id)
            .bind(afd_runner::sql::event_type::RUN_ACQUIRED)
            .bind(sqlx::types::Json(metadata))
            .bind(None::<sqlx::types::Json<serde_json::Value>>)
            .bind(issue.held_until.to_sqlx())
            .bind(issue.run_version)
            .bind(issue.owner.kind().as_str())
            .bind(i64::try_from(issue.max_runtime.as_secs()).unwrap_or(i64::MAX))
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        Ok(created.map(|_created| run_id))
    }
}

/// What [`RunService::issue_draft`] writes, by name.
struct Issue<'a> {
    runner: &'a Uuid7,
    owner: &'a RunOwner,
    turn: &'a SetupTurn,
    fence: i64,
    held_until: Timestamp,
    run_version: i64,
    reassignment: bool,
    max_runtime: std::time::Duration,
}

impl DraftAssignment {
    /// What the session keeps beside the assignment.
    #[must_use]
    pub fn assigned(&self) -> AssignedRun {
        AssignedRun {
            run_id: self.run_id.clone(),
            expires_at: self.expires_at,
            target: None,
            reported: None,
            draft: Some(DraftTarget {
                draft_id: self.turn.draft_id.clone(),
            }),
        }
    }

    /// The assignment: a [`RunPayload`] the draft owns, borrowing the key this
    /// value holds.
    #[must_use]
    pub fn payload(&self) -> RunPayload<'_> {
        let turn = &self.turn;
        let model = &turn.model;
        let candidate = turn.candidate.as_ref().map(|head| SetupCandidate {
            id: Cow::Owned(head.id.to_string()),
            revision: head.revision,
            definition: head.definition.clone(),
        });
        let instructions = self
            .tool
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let context_tokens = u32::try_from(model.context_window_tokens).unwrap_or_default();
        let request = serde_json::json!({ (af_wire::turn::MESSAGE): turn.user_text });
        let (history, history_sequences) = history(turn);
        RunPayload {
            run_id: Cow::Owned(self.run_id.to_string()),
            fencing_token: u64::try_from(self.fence).unwrap_or_default(),
            setup_revision_id: None,
            turn_resume: None,
            run_expires_at: self.expires_at,
            event: TurnEnvelope {
                turn_id: Cow::Owned(turn.message_id.to_string()),
                // A draft's run names its draft where a fleet's names its fleet.
                fleet_id: Cow::Owned(turn.draft_id.to_string()),
                workspace_id: Cow::Owned(turn.workspace_id.to_string()),
                actor: Cow::Owned(turn.user_id.to_string()),
                event_type: TurnKind::Chat,
                request_json: Cow::Owned(request.to_string()),
                created_at: turn.message_created_at,
            },
            policy: ExecutionPolicy {
                network_policy: NetworkPolicy::default(),
                tools: vec![Cow::Borrowed(PROPOSE_SETUP)],
                secrets_map: None,
                mintable: Vec::new(),
                provider: Cow::Borrowed(&model.provider),
                inference: model.inference.clone(),
                model_context_tokens: context_tokens,
                api_key: Cow::Borrowed(self.api_key.expose()),
                inference_host: Cow::Borrowed(&self.inference_host),
                base_url: model.base_url.as_deref().map(Cow::Borrowed),
                repository_binding: None,
                http_origin_policies: Vec::new(),
                context: afd_gate::policy::context::resolve(
                    None,
                    afd_gate::policy::context::Overlay {
                        cap_tokens: context_tokens,
                        model: &model.model,
                    },
                ),
            },
            instructions: Cow::Borrowed(instructions),
            history,
            resume_hold: false,
            decision: None,
            task: None,
            durable_tasks: false,
            memory: None,
            inputs_waiting: false,
            owner: Some(Owner::Draft {
                draft_id: Cow::Owned(turn.draft_id.to_string()),
            }),
            setup: Some(SetupWork {
                sequence: u64::try_from(turn.sequence).unwrap_or_default(),
                candidate,
                save_requested: turn.save_requested,
                tool: self.tool.clone(),
                history_sequences,
            }),
        }
    }
}

/// The draft's earlier turns, oldest first, and each one's sequence, within a
/// fleet chat's caps: each text cut at [`TURN_TEXT_BYTES_MAX`] on a character
/// boundary, and the oldest dropped until the rest fit [`HISTORY_BYTES_MAX`].
fn history(turn: &SetupTurn) -> (Vec<Turn<'_>>, Vec<u64>) {
    let cut = |text: &str| -> String {
        text.split_at(text.floor_char_boundary(TURN_TEXT_BYTES_MAX))
            .0
            .to_owned()
    };
    let turns: Vec<(Turn<'_>, u64)> = turn
        .history
        .iter()
        .map(|earlier| {
            let text = Turn {
                message: Cow::Owned(cut(&earlier.message)),
                answer: Cow::Owned(cut(&earlier.answer)),
            };
            (text, u64::try_from(earlier.sequence).unwrap_or_default())
        })
        .collect();
    let mut total: usize = turns
        .iter()
        .map(|(t, _)| t.message.len() + t.answer.len())
        .sum();
    turns
        .into_iter()
        .skip_while(|(t, _)| {
            let over = total > HISTORY_BYTES_MAX;
            if over {
                total -= t.message.len() + t.answer.len();
            }
            over
        })
        .unzip()
}
