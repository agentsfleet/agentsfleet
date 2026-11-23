//! Recovery reads original effect identities before any further action or judgment.

use super::Harness;
use crate::{Result, decision::unavailable};
use af_wire::durable_task::{
    DurableTaskEffect, DurableTaskEffectState, DurableTaskEffectsQuery, DurableTaskInputsQuery,
};

impl Harness<'_, '_> {
    pub(super) async fn reconcile_durable_task_effects(&mut self) -> Result<()> {
        let Some(prepared) = self.task.as_mut().filter(|task| task.allow_actions) else {
            return Ok(());
        };
        let durable_task_id = prepared
            .inputs
            .current_durable_task
            .as_ref()
            .ok_or_else(unavailable)?
            .durable_task_id
            .to_string();
        let verbs = self.shared.run.verbs;
        // Each task has a cumulative publication limit. The bounded page walk
        // includes refused attempts as well as confirmations and uncertainties.
        for page_number in 0..64 {
            if self.stop.is_cancelled() {
                return Err(unavailable());
            }
            let page = verbs
                .durable_task_effects(DurableTaskEffectsQuery {
                    fencing_token: self.shared.fencing_token,
                    durable_task_id: durable_task_id.clone(),
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
                let effect = reread_unsettled(verbs, previous).await?;
                if blocks_publication(effect.state, effect.commit.is_some()) {
                    return Err(unavailable());
                }
                if matches!(effect.state, DurableTaskEffectState::Confirmed) {
                    self.confirmed_effect_watermark = effect.ordinal;
                }
                self.transcript.push(guffbolt::Message::User(
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
            .durable_task_inputs(DurableTaskInputsQuery {
                fencing_token: self.shared.fencing_token,
                starting_after: None,
                after_input_sequence: Some(self.incorporated_input_sequence),
                limit: 4,
            })
            .await
            .map_err(crate::decision::native)?;
        let current = inputs
            .current_durable_task
            .as_mut()
            .ok_or_else(unavailable)?;
        if current.durable_task_id != durable_task_id
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

/// Reads back an effect whose outcome the page left unsettled, refusing one
/// whose identity, ordinal or tested content moved.
async fn reread_unsettled(
    verbs: &dyn afr_tools::RunVerbs,
    previous: DurableTaskEffect,
) -> Result<DurableTaskEffect> {
    if !matches!(
        previous.state,
        DurableTaskEffectState::Prepared
            | DurableTaskEffectState::Issued
            | DurableTaskEffectState::Unknown
    ) {
        return Ok(previous);
    }
    let recovered = verbs
        .durable_task_effect(&previous.effect_id)
        .await
        .map_err(crate::decision::native)?;
    if recovered.effect_id != previous.effect_id
        || recovered.ordinal != previous.ordinal
        || recovered.expected_head != previous.expected_head
        || recovered.tested_tree != previous.tested_tree
    {
        return Err(unavailable());
    }
    Ok(recovered)
}

/// Whether an effect in `state` blocks further publication: anything but a
/// confirmation with its commit or a refusal without one. An unknown remote
/// outcome cannot become a fresh operation after replacement.
const fn blocks_publication(state: DurableTaskEffectState, committed: bool) -> bool {
    match state {
        DurableTaskEffectState::Confirmed => !committed,
        DurableTaskEffectState::Refused => committed,
        DurableTaskEffectState::Prepared
        | DurableTaskEffectState::Issued
        | DurableTaskEffectState::Unknown => true,
    }
}
