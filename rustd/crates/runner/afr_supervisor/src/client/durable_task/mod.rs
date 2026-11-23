//! Typed task calls share the ordinary runner transport and never redispatch effects.

use super::{Body, ControlPlane, Verb};
use crate::Result;
use af_core::id::Uuid7;
use af_wire::{
    durable_task::{
        DecisionReceiptInput, DecisionReceiptResult, DecisionReservationInput,
        DurableTaskBindingsInput, DurableTaskBindingsResponse, DurableTaskEffect,
        DurableTaskEffectInput, DurableTaskEffectsPage, DurableTaskEffectsQuery,
        DurableTaskInputReceiptInput, DurableTaskInputReceiptResult, DurableTaskInputsQuery,
        DurableTaskInputsResponse,
    },
    paths,
};
use std::borrow::Cow;
use url::form_urlencoded::Serializer;

mod checkpoint;

impl ControlPlane {
    /// Native input reads do not acknowledge or consume any message.
    pub(crate) async fn durable_task_inputs(
        &self,
        run: &Uuid7,
        query: &DurableTaskInputsQuery,
    ) -> Result<DurableTaskInputsResponse<'static>> {
        let path = queried(
            path(paths::RUN_DURABLE_TASK_INPUTS, run),
            query.fencing_token,
            |encoded| {
                encoded.append_pair("limit", &query.limit.to_string());
                if let Some(after) = &query.starting_after {
                    encoded.append_pair("starting_after", after);
                }
                if let Some(sequence) = query.after_input_sequence {
                    encoded.append_pair("after_input_sequence", &sequence.to_string());
                }
            },
        );
        Ok(self
            .send(Verb::DurableTaskInputs, path, None)
            .await?
            .decode::<DurableTaskInputsResponse<'_>>()?
            .into_owned())
    }

    /// One complete interpretation, replayed only by its native receipt identity.
    pub(crate) async fn durable_task_bindings(
        &self,
        run: &Uuid7,
        input: &DurableTaskBindingsInput,
    ) -> Result<DurableTaskBindingsResponse<'static>> {
        Ok(self
            .send_json(
                Verb::DurableTaskBindings,
                path(paths::RUN_DURABLE_TASK_BINDINGS, run),
                input,
            )
            .await?
            .decode::<DurableTaskBindingsResponse<'_>>()?
            .into_owned())
    }

    /// The native recorder distinguishes application, queueing and answered text.
    pub(crate) async fn durable_task_input_receipt(
        &self,
        run: &Uuid7,
        input: &DurableTaskInputReceiptInput,
    ) -> Result<DurableTaskInputReceiptResult> {
        self.send_json(
            Verb::DurableTaskInputReceipt,
            path(paths::RUN_DURABLE_TASK_INPUT_RECEIPTS, run),
            input,
        )
        .await?
        .decode()
    }

    /// Keep dispatch bytes in the existing wiping response owner until consumed.
    pub(crate) async fn durable_task_decision_reserve(
        &self,
        run: &Uuid7,
        input: &DecisionReservationInput,
    ) -> Result<Body> {
        self.send_json(
            Verb::DurableTaskDecisionReserve,
            path(paths::RUN_DURABLE_TASK_DECISIONS, run),
            input,
        )
        .await
    }

    /// Original-owner receipt recording is valid independently of task execution.
    pub(crate) async fn durable_task_decision_record(
        &self,
        run: &Uuid7,
        call: &Uuid7,
        input: &DecisionReceiptInput,
    ) -> Result<DecisionReceiptResult> {
        let path = member(paths::RUN_DURABLE_TASK_DECISION, run, "{call_id}", call);
        self.send_json(Verb::DurableTaskDecisionRecord, path, input)
            .await?
            .decode()
    }

    /// A physical publication has one native operation identity and no hidden retry.
    pub(crate) async fn durable_task_publish(
        &self,
        run: &Uuid7,
        input: &DurableTaskEffectInput,
    ) -> Result<DurableTaskEffect> {
        self.send_json(
            Verb::DurableTaskEffectPublish,
            path(paths::RUN_DURABLE_TASK_EFFECTS, run),
            input,
        )
        .await?
        .decode()
    }

    /// Recover saved native effects rather than repeating a lost mutation.
    pub(crate) async fn durable_task_effects(
        &self,
        run: &Uuid7,
        query: &DurableTaskEffectsQuery,
    ) -> Result<DurableTaskEffectsPage> {
        let path = queried(
            path(paths::RUN_DURABLE_TASK_EFFECTS, run),
            query.fencing_token,
            |encoded| {
                encoded.append_pair("durable_task_id", &query.durable_task_id);
                encoded.append_pair("ordinal_after", &query.ordinal_after.to_string());
                encoded.append_pair("limit", &query.limit.to_string());
                if let Some(after) = &query.starting_after {
                    encoded.append_pair("starting_after", after);
                }
            },
        );
        self.send(Verb::DurableTaskEffects, path, None)
            .await?
            .decode()
    }

    /// A known native identity can be reconciled without another write.
    pub(crate) async fn durable_task_effect(
        &self,
        run: &Uuid7,
        effect: &Uuid7,
        fence: u64,
    ) -> Result<DurableTaskEffect> {
        let path = queried(
            member(paths::RUN_DURABLE_TASK_EFFECT, run, "{effect_id}", effect),
            fence,
            |_| {},
        );
        self.send(Verb::DurableTaskEffect, path, None)
            .await?
            .decode()
    }
}

fn path(template: &str, run: &Uuid7) -> Cow<'static, str> {
    Cow::Owned(template.replace("{run_id}", run.as_str()))
}

fn member(template: &str, run: &Uuid7, parameter: &str, id: &Uuid7) -> Cow<'static, str> {
    Cow::Owned(path(template, run).replace(parameter, id.as_str()))
}

fn queried(
    path: Cow<'static, str>,
    fence: u64,
    extend: impl FnOnce(&mut Serializer<'_, String>),
) -> Cow<'static, str> {
    let mut encoded = Serializer::new(String::new());
    encoded.append_pair(
        af_wire::schedule_verb::QUERY_FENCING_TOKEN,
        &fence.to_string(),
    );
    extend(&mut encoded);
    let mut path = path.into_owned();
    path.push('?');
    path.push_str(&encoded.finish());
    Cow::Owned(path)
}
