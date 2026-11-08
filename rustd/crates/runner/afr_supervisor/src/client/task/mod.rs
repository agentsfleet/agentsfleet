//! Typed task calls share the ordinary runner transport and never redispatch effects.

use super::{Body, ControlPlane, Verb};
use crate::Result;
use af_core::id::Uuid7;
use af_wire::{
    paths,
    task::{
        DecisionReceiptInput, DecisionReceiptResult, DecisionReservationInput, TaskBindingsInput,
        TaskBindingsResponse, TaskEffect, TaskEffectInput, TaskEffectsPage, TaskEffectsQuery,
        TaskInputReceiptInput, TaskInputReceiptResult, TaskInputsQuery, TaskInputsResponse,
    },
};
use std::borrow::Cow;
use url::form_urlencoded::Serializer;

mod checkpoint;

impl ControlPlane {
    /// Native input reads do not acknowledge or consume any message.
    pub(crate) async fn task_inputs(
        &self,
        lease: &Uuid7,
        query: &TaskInputsQuery,
    ) -> Result<TaskInputsResponse<'static>> {
        let path = queried(
            path(paths::LEASE_TASK_INPUTS, lease),
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
            .send(Verb::TaskInputs, path, None)
            .await?
            .decode::<TaskInputsResponse<'_>>()?
            .into_owned())
    }

    /// One complete interpretation, replayed only by its native receipt identity.
    pub(crate) async fn task_bindings(
        &self,
        lease: &Uuid7,
        input: &TaskBindingsInput,
    ) -> Result<TaskBindingsResponse<'static>> {
        Ok(self
            .send_json(
                Verb::TaskBindings,
                path(paths::LEASE_TASK_BINDINGS, lease),
                input,
            )
            .await?
            .decode::<TaskBindingsResponse<'_>>()?
            .into_owned())
    }

    /// The native recorder distinguishes application, queueing and answered text.
    pub(crate) async fn task_input_receipt(
        &self,
        lease: &Uuid7,
        input: &TaskInputReceiptInput,
    ) -> Result<TaskInputReceiptResult> {
        self.send_json(
            Verb::TaskInputReceipt,
            path(paths::LEASE_TASK_INPUT_RECEIPTS, lease),
            input,
        )
        .await?
        .decode()
    }

    /// Keep dispatch bytes in the existing wiping response owner until consumed.
    pub(crate) async fn task_decision_reserve(
        &self,
        lease: &Uuid7,
        input: &DecisionReservationInput,
    ) -> Result<Body> {
        self.send_json(
            Verb::TaskDecisionReserve,
            path(paths::LEASE_TASK_DECISIONS, lease),
            input,
        )
        .await
    }

    /// Original-owner receipt recording is valid independently of task execution.
    pub(crate) async fn task_decision_record(
        &self,
        lease: &Uuid7,
        call: &Uuid7,
        input: &DecisionReceiptInput,
    ) -> Result<DecisionReceiptResult> {
        let path = member(paths::LEASE_TASK_DECISION, lease, "{call_id}", call);
        self.send_json(Verb::TaskDecisionRecord, path, input)
            .await?
            .decode()
    }

    /// A physical publication has one native operation identity and no hidden retry.
    pub(crate) async fn task_publish(
        &self,
        lease: &Uuid7,
        input: &TaskEffectInput,
    ) -> Result<TaskEffect> {
        self.send_json(
            Verb::TaskEffectPublish,
            path(paths::LEASE_TASK_EFFECTS, lease),
            input,
        )
        .await?
        .decode()
    }

    /// Recover saved native effects rather than repeating a lost mutation.
    pub(crate) async fn task_effects(
        &self,
        lease: &Uuid7,
        query: &TaskEffectsQuery,
    ) -> Result<TaskEffectsPage> {
        let path = queried(
            path(paths::LEASE_TASK_EFFECTS, lease),
            query.fencing_token,
            |encoded| {
                encoded.append_pair("task_id", &query.task_id);
                encoded.append_pair("ordinal_after", &query.ordinal_after.to_string());
                encoded.append_pair("limit", &query.limit.to_string());
                if let Some(after) = &query.starting_after {
                    encoded.append_pair("starting_after", after);
                }
            },
        );
        self.send(Verb::TaskEffects, path, None).await?.decode()
    }

    /// A known native identity can be reconciled without another write.
    pub(crate) async fn task_effect(
        &self,
        lease: &Uuid7,
        effect: &Uuid7,
        fence: u64,
    ) -> Result<TaskEffect> {
        let path = queried(
            member(paths::LEASE_TASK_EFFECT, lease, "{effect_id}", effect),
            fence,
            |_| {},
        );
        self.send(Verb::TaskEffect, path, None).await?.decode()
    }
}

fn path(template: &str, lease: &Uuid7) -> Cow<'static, str> {
    Cow::Owned(template.replace("{lease_id}", lease.as_str()))
}

fn member(template: &str, lease: &Uuid7, parameter: &str, id: &Uuid7) -> Cow<'static, str> {
    Cow::Owned(path(template, lease).replace(parameter, id.as_str()))
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
