//! Recovery reads original effect identities before any further action or judgment.

use super::Harness;
use crate::{Result, decision::unavailable};
use afd_wire::task::{TaskEffectState, TaskEffectsQuery, TaskInputsQuery};

impl Harness<'_, '_> {
    pub(super) async fn reconcile_task_effects(&mut self) -> Result<()> {
        let Some(prepared) = self.task.as_mut().filter(|task| task.allow_actions) else {
            return Ok(());
        };
        let task_id = prepared
            .inputs
            .current_task
            .as_ref()
            .ok_or_else(unavailable)?
            .task_id
            .to_string();
        let verbs = self.shared.lease.verbs;
        // Each task has a cumulative publication limit. The bounded page walk
        // includes refused attempts as well as confirmations and uncertainties.
        for page_number in 0..64 {
            if self.stop.is_cancelled() {
                return Err(unavailable());
            }
            let page = verbs
                .task_effects(TaskEffectsQuery {
                    fencing_token: self.shared.fencing_token,
                    task_id: task_id.clone(),
                    ordinal_after: self.reconciled_effect_ordinal,
                    starting_after: None,
                    limit: 50,
                })
                .await
                .map_err(crate::decision::native)?;
            for previous in page.effects {
                if previous.ordinal
                    != self
                        .reconciled_effect_ordinal
                        .checked_add(1)
                        .ok_or_else(unavailable)?
                {
                    return Err(unavailable());
                }
                let effect = if matches!(
                    previous.state,
                    TaskEffectState::Prepared | TaskEffectState::Issued | TaskEffectState::Unknown
                ) {
                    let recovered = verbs
                        .task_effect(&previous.effect_id)
                        .await
                        .map_err(crate::decision::native)?;
                    if recovered.effect_id != previous.effect_id
                        || recovered.ordinal != previous.ordinal
                        || recovered.expected_head != previous.expected_head
                        || recovered.tested_tree != previous.tested_tree
                    {
                        return Err(unavailable());
                    }
                    recovered
                } else {
                    previous
                };
                match effect.state {
                    TaskEffectState::Confirmed if effect.commit.is_some() => {
                        self.confirmed_effect_watermark = effect.ordinal;
                    }
                    TaskEffectState::Refused if effect.commit.is_none() => {}
                    // An unknown remote outcome blocks further publication. It
                    // cannot become a fresh operation after replacement.
                    _ => return Err(unavailable()),
                }
                self.messages.push(afr_providers::Message::User(
                    self.shared
                        .scrub
                        .clean(
                            serde_json::json!({ "native_publication_receipt": effect }).to_string(),
                        )
                        .into_inner(),
                ));
                self.reconciled_effect_ordinal = effect.ordinal;
            }
            if page.next_cursor.is_none() {
                break;
            }
            if page_number == 63 {
                return Err(unavailable());
            }
        }
        let mut inputs = verbs
            .task_inputs(TaskInputsQuery {
                fencing_token: self.shared.fencing_token,
                starting_after: None,
                after_input_sequence: Some(self.incorporated_input_sequence),
                limit: 4,
            })
            .await
            .map_err(crate::decision::native)?;
        let current = inputs.current_task.as_mut().ok_or_else(unavailable)?;
        if current.task_id != task_id
            || current.confirmed_effect_watermark != self.confirmed_effect_watermark
            || current.effect_cursor != self.reconciled_effect_ordinal
            || current.covered_input_sequence != self.incorporated_input_sequence
            || current.input_sequence != self.incorporated_input_sequence
        {
            return Err(unavailable());
        }
        prepared.execution_deadline = inputs.execution_deadline;
        prepared.inputs = inputs;
        Ok(())
    }
}
