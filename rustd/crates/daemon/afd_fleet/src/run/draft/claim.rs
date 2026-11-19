//! The draft claim: one transaction under the host lock takes the draft, its
//! slot and its run, and the offer is rendered from what it committed.
//!
//! Lock order is the host row, then the draft's creator and draft (`NOWAIT`,
//! so a busy draft never stalls the walk), then the draft's slot and its
//! earlier run. A draft's report takes the draft and then the slot, and holds
//! no host row, so the two never wait on each other.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_crypto::secret::SecretString;
use af_wire::{
    authoring::FailureReason,
    event::{EventEnvelope, EventType},
    policy::{ExecutionPolicy, NetworkPolicy},
    run::{
        HISTORY_BYTES_MAX, Owner, RunKind, RunPayload, SetupCandidate, SetupWork,
        TURN_TEXT_BYTES_MAX, Turn,
    },
};
use afd_billing::rates::Posture;
use afd_runner::capacity::choice::Ticket;
use afd_setup::setup_run::{SetupAdmission, SetupHint, SetupOwed, SetupTurn, setup_tool};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::Connection as _;

use super::DraftTarget;
use crate::error::{Result, query};
use crate::run::{Offered, RunOwner, RunService, sql};

/// What query failures name the draft claim as.
const CONTEXT: &str = "claim a draft run";

/// The one tool a setup run offers.
const PROPOSE_SETUP: &str = "propose_setup";

/// A committed draft run, ready to render as its offer.
///
/// Holds the key the offer carries, so the payload borrows it rather than
/// copying a secret into an owned string.
#[derive(Debug)]
pub struct DraftOffer {
    run_id: Uuid7,
    fence: i64,
    expires_at: Timestamp,
    turn: SetupTurn,
    api_key: SecretString,
    inference_host: Box<str>,
    tool: serde_json::Value,
}

/// [`sql::shared::CLAIM_SLOT`]'s winning row, by column name.
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
    earlier_runs: i64,
}

/// What one candidate's claim came to.
enum Placement {
    Issued(Box<DraftOffer>),
    /// The host can take no more work this pass.
    Busy,
    /// This draft yields nothing now; the next may.
    Skipped,
}

impl RunService {
    /// Claims the oldest waiting draft turn `runner` may run, under the
    /// pass's placement `ticket`, and commits its run.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a key that would not open.
    pub async fn claim_draft(
        &self,
        drafts: &afd_setup::Drafts,
        runner: &Uuid7,
        ticket: Ticket,
        now: Timestamp,
    ) -> Result<Option<DraftOffer>> {
        for hint in drafts.setup_candidates(runner).await? {
            match self.claim_one(drafts, runner, &hint, ticket, now).await? {
                Placement::Issued(offer) => return Ok(Some(*offer)),
                Placement::Busy => return Ok(None),
                Placement::Skipped => {}
            }
        }
        Ok(None)
    }

    async fn claim_one(
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
        if !host.can_claim() || !ticket.permits_revision(host.run_version(), RunKind::Authoring) {
            return Ok(Placement::Busy);
        }
        // Before any write: a foreign runner leaves no run and no failure behind.
        if !host.serves() {
            afd_runner::capacity::refused_placement(
                runner,
                afd_runner::capacity::PlacementRefusal::ForeignTenant,
                RunKind::Authoring.as_str(),
            );
            return Ok(Placement::Skipped);
        }
        let mut owed = SetupOwed::default();
        let turn = match drafts
            .admit_setup_turn(&mut tx, hint, host.tenant_owned(), &mut owed)
            .await?
        {
            SetupAdmission::Admitted(turn) => *turn,
            SetupAdmission::Passed | SetupAdmission::Busy => {
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
        let Some((won, reclaim)) = self
            .take_slot(&mut tx, drafts, runner, &turn, &mut owed)
            .await?
        else {
            tx.commit().await.map_err(query(CONTEXT))?;
            owed.settle(drafts);
            return Ok(Placement::Skipped);
        };
        let issue = Issue {
            runner,
            owner: &owner,
            turn: &turn,
            fence: won.fencing_token,
            held_until: won.run_expires_at,
            run_version: host.run_version(),
            reclaim,
            max_runtime: drafts.setup_max_runtime(),
        };
        let Some(run_id) = self.issue_draft(&mut tx, issue, now).await? else {
            // The host filled, the fence or the choice moved: nothing commits.
            return Ok(Placement::Busy);
        };
        drafts.start_setup_turn(&mut tx, &turn).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        owed.settle(drafts);
        let tool = setup_tool(turn.candidate.is_some());
        Ok(Placement::Issued(Box::new(DraftOffer {
            run_id,
            fence: won.fencing_token,
            expires_at: won.run_expires_at,
            turn,
            api_key: route.api_key,
            inference_host: route.inference_host,
            tool,
        })))
    }

    /// Wins the draft's slot, bumping its fence, and expires the lapsed run
    /// that ran the turn before: the slot and whether this claim reoffers the
    /// turn. `None` when a live runner still holds the slot, or when the turn's
    /// runs lapsed [`af_core::limits::EVENT_RUNS_MAX`] times, which fails
    /// it and frees the slot rather than reoffering it forever.
    async fn take_slot(
        &self,
        tx: &mut sqlx::PgConnection,
        drafts: &afd_setup::Drafts,
        runner: &Uuid7,
        turn: &SetupTurn,
        owed: &mut SetupOwed,
    ) -> Result<Option<(Won, bool)>> {
        let owner = RunOwner::Draft(turn.draft_id.clone());
        let won: Option<Won> = sqlx::query_as(sql::shared::CLAIM_SLOT)
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
            return Ok(None);
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
        if prior.earlier_runs < i64::from(af_core::limits::EVENT_RUNS_MAX) {
            return Ok(Some((won, prior.predecessor_run_id.is_some())));
        }
        // Every earlier run lapsed unreported: the turn is lost, not retried forever.
        drafts
            .fail_setup_turn(&mut *tx, turn, FailureReason::ProviderUsageUnknown, owed)
            .await?;
        sqlx::query(sql::shared::RELEASE_UNLEASED_SLOT)
            .bind(owner.slot_id())
            .bind(won.fencing_token)
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        Ok(None)
    }

    /// Writes the draft's run under its claim's fence with
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
        let kind = if issue.reclaim { "reclaim" } else { "fresh" };
        let metadata = std::collections::BTreeMap::from([
            (afd_runner::sql::meta::RUN_ID, run_id.as_str()),
            (
                afd_runner::sql::meta::AGENTSFLEET_EVENT_ID,
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
    reclaim: bool,
    max_runtime: std::time::Duration,
}

impl DraftOffer {
    /// What the session keeps beside the offer.
    #[must_use]
    pub fn offered(&self) -> Offered {
        Offered {
            run_id: self.run_id.clone(),
            expires_at: self.expires_at,
            target: None,
            reported: None,
            draft: Some(DraftTarget {
                draft_id: self.turn.draft_id.clone(),
            }),
        }
    }

    /// The offer: a [`RunPayload`] the draft owns, borrowing the key this
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
        let request = serde_json::json!({ (af_wire::event::MESSAGE): turn.user_text });
        RunPayload {
            run_id: Cow::Owned(self.run_id.to_string()),
            fencing_token: u64::try_from(self.fence).unwrap_or_default(),
            setup_revision_id: None,
            turn_resume: None,
            run_expires_at: self.expires_at,
            event: EventEnvelope {
                event_id: Cow::Owned(turn.message_id.to_string()),
                // A draft's run names its draft where a fleet's names its fleet.
                fleet_id: Cow::Owned(turn.draft_id.to_string()),
                workspace_id: Cow::Owned(turn.workspace_id.to_string()),
                actor: Cow::Owned(turn.user_id.to_string()),
                event_type: EventType::Chat,
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
            history: history(turn),
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
            }),
        }
    }
}

/// The draft's earlier turns, oldest first, within a fleet chat's caps: each
/// text cut at [`TURN_TEXT_BYTES_MAX`] on a character boundary, and the oldest
/// dropped until the rest fit [`HISTORY_BYTES_MAX`].
fn history(turn: &SetupTurn) -> Vec<Turn<'_>> {
    let cut = |text: &str| -> String {
        text.split_at(text.floor_char_boundary(TURN_TEXT_BYTES_MAX))
            .0
            .to_owned()
    };
    let turns: Vec<Turn<'_>> = turn
        .history
        .iter()
        .map(|earlier| Turn {
            message: Cow::Owned(cut(&earlier.message)),
            answer: Cow::Owned(cut(&earlier.answer)),
        })
        .collect();
    let mut total: usize = turns.iter().map(|t| t.message.len() + t.answer.len()).sum();
    turns
        .into_iter()
        .skip_while(|t| {
            let over = total > HISTORY_BYTES_MAX;
            if over {
                total -= t.message.len() + t.answer.len();
            }
            over
        })
        .collect()
}
