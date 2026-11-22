//! Checked conversion of stored task state to its shared protocol context.

use af_core::id::Uuid7;
use std::borrow::Cow;

use af_wire::durable_task::{
    DurableTaskContext, DurableTaskInputAction, DurableTaskLimits, DurableTaskState,
    DurableTaskUsage,
};
use serde::Deserialize;

use super::DurableTaskSnapshot;
use crate::error::{Error, ErrorKind, Result};

#[derive(Debug, sqlx::FromRow)]
pub(super) struct DurableTaskRow {
    pub id: Uuid7,
    pub originating_item_id: Uuid7,
    pub target: String,
    pub dependency: Option<String>,
    pub publication: Option<String>,
    pub accepted_revision_id: Uuid7,
    pub accepted_revision_number: i64,
    pub title: String,
    pub objective: String,
    pub questions: String,
    pub goal_digest: String,
    pub state: String,
    pub resource_version: i64,
    pub generation: i64,
    pub stop_generation: i64,
    pub covered_input_sequence: i64,
    pub input_sequence: i64,
    pub checkpoint_id: Option<Uuid7>,
    pub checkpoint_generation: Option<i64>,
    pub effect_cursor: i64,
    pub confirmed_effect_watermark: i64,
    pub max_model_nanos: i64,
    pub used_model_nanos: i64,
    pub max_running_nanos: i64,
    pub used_running_nanos: i64,
    pub max_runs: i32,
    pub used_runs: i32,
    pub max_publications: i32,
    pub used_publications: i32,
    pub max_decision_calls: i32,
    pub used_decision_calls: i32,
    pub wall_seconds: i32,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub expires_at: jiff::Timestamp,
}

impl DurableTaskRow {
    pub(super) fn snapshot(self) -> Result<DurableTaskSnapshot> {
        let state = state(&self.state)?;
        let questions =
            serde_json::from_str(&self.questions).map_err(af_core::json::Unreadable::from)?;
        Ok(DurableTaskSnapshot {
            publication: self
                .publication
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(af_core::json::Unreadable::from)?,
            publication_digest: self
                .publication
                .as_deref()
                .map(|value| af_core::etag::sha256_hex(value.as_bytes())),
            dependency: self
                .dependency
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(af_core::json::Unreadable::from)?,
            dependency_digest: self
                .dependency
                .as_deref()
                .map(|value| af_core::etag::sha256_hex(value.as_bytes())),
            title: self.title,
            resource_version: counter(self.resource_version, "resource_version")?,
            accepted_revision_number: counter(self.accepted_revision_number, "revision")?,
            context: DurableTaskContext {
                durable_task_id: Cow::Owned(self.id.into()),
                originating_item_id: Cow::Owned(self.originating_item_id.into()),
                target: target(&self.target)?,
                generation: counter(self.generation, "generation")?,
                stop_generation: counter(self.stop_generation, "stop_generation")?,
                accepted_revision_id: Cow::Owned(self.accepted_revision_id.into()),
                objective: Cow::Owned(self.objective),
                goal_digest: Cow::Owned(self.goal_digest),
                questions,
                state,
                covered_input_sequence: counter(
                    self.covered_input_sequence,
                    "covered_input_sequence",
                )?,
                input_sequence: counter(self.input_sequence, "input_sequence")?,
                effect_cursor: counter(self.effect_cursor, "effect_cursor")?,
                confirmed_effect_watermark: counter(
                    self.confirmed_effect_watermark,
                    "confirmed_effect_watermark",
                )?,
                checkpoint_id: self.checkpoint_id.map(|id| Cow::Owned(id.into())),
                checkpoint_generation: optional_counter(
                    self.checkpoint_generation,
                    "checkpoint generation",
                )?,
                expires_at: self.expires_at,
                limits: DurableTaskLimits {
                    model_nanos: self.max_model_nanos,
                    wall_seconds: small_counter(self.wall_seconds, "wall_seconds")?,
                    running_seconds: small_counter(
                        self.max_running_nanos / 1_000_000_000,
                        "max_running_nanos",
                    )?,
                    runs: small_counter(self.max_runs, "max_runs")?,
                    publications: small_counter(self.max_publications, "max_publications")?,
                    decision_calls: small_counter(self.max_decision_calls, "max_decision_calls")?,
                },
                usage: DurableTaskUsage {
                    model_nanos: self.used_model_nanos,
                    running_nanos: counter(self.used_running_nanos, "used_running_nanos")?,
                    runs: small_counter(self.used_runs, "used_runs")?,
                    publications: small_counter(self.used_publications, "used_publications")?,
                    decision_calls: small_counter(self.used_decision_calls, "used_decision_calls")?,
                },
            },
        })
    }
}

fn target(encoded: &str) -> Result<af_wire::durable_task::DurableTaskTarget> {
    let value: serde_json::Value =
        serde_json::from_str(encoded).map_err(af_core::json::Unreadable::from)?;
    if value.get("kind").and_then(serde_json::Value::as_str) == Some("api_only") {
        return Ok(af_wire::durable_task::DurableTaskTarget::ApiOnly);
    }
    let target: super::github::BoundPr =
        serde_json::from_value(value).map_err(af_core::json::Unreadable::from)?;
    Ok(af_wire::durable_task::DurableTaskTarget::ExistingPr {
        repository: target.repository,
        repository_id: target.repository_id,
        number: target.number,
        branch: target.branch,
        base: target.base,
        head: target.head,
        tree: target.tree,
        url: target.url,
        workflows: target
            .workflows
            .into_iter()
            .map(|workflow| af_wire::durable_task::DurableTaskWorkflow {
                id: workflow.id,
                path: workflow.path,
                digest: workflow.digest,
            })
            .collect(),
    })
}

pub(super) fn state(stored: &str) -> Result<DurableTaskState> {
    DurableTaskState::deserialize(serde::de::value::StrDeserializer::new(stored)).map_err(
        |source: serde::de::value::Error| Error::from(ErrorKind::DurableTaskState { source }),
    )
}

/// A stored input action, read back through the wire spelling it was written with.
pub(super) fn action(stored: &str) -> Result<DurableTaskInputAction> {
    DurableTaskInputAction::deserialize(serde::de::value::StrDeserializer::new(stored)).map_err(
        |source: serde::de::value::Error| Error::from(ErrorKind::DurableTaskState { source }),
    )
}

/// A nullable stored counter: absent stays absent, present is checked as [`counter`] checks it.
pub(super) fn optional_counter(stored: Option<i64>, column: &'static str) -> Result<Option<u64>> {
    stored.map(|value| counter(value, column)).transpose()
}

pub(super) fn counter<T>(stored: T, column: &'static str) -> Result<u64>
where
    u64: TryFrom<T, Error = std::num::TryFromIntError>,
{
    u64::try_from(stored)
        .map_err(|source| Error::from(ErrorKind::DurableTaskCounter { column, source }))
}

pub(super) fn small_counter<T>(stored: T, column: &'static str) -> Result<u32>
where
    u32: TryFrom<T, Error = std::num::TryFromIntError>,
{
    u32::try_from(stored)
        .map_err(|source| Error::from(ErrorKind::DurableTaskCounter { column, source }))
}

pub(super) fn sql_counter<T>(value: T, column: &'static str) -> Result<i64>
where
    i64: TryFrom<T, Error = std::num::TryFromIntError>,
{
    i64::try_from(value)
        .map_err(|source| Error::from(ErrorKind::DurableTaskCounter { column, source }))
}

pub(super) fn sql_small_counter<T>(value: T, column: &'static str) -> Result<i32>
where
    i32: TryFrom<T, Error = std::num::TryFromIntError>,
{
    i32::try_from(value)
        .map_err(|source| Error::from(ErrorKind::DurableTaskCounter { column, source }))
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test: an unmet precondition should fail the test loudly"
)]
mod tests {
    use af_core::error_code;
    use af_wire::durable_task::DurableTaskInputAction;

    use super::{action, optional_counter};

    // Value: protects=a stored input action reads back as the action it was
    // accepted as, and an unknown spelling is a datastore fault, never a guess;
    // fails_when=the decoder maps an unknown spelling to a default action or a
    // caller-facing refusal; why_new=three readers shared a copied decoder, now
    // this one; seam=the row decoder every input reader calls.
    #[test]
    fn a_stored_action_reads_back_and_an_unknown_spelling_is_a_datastore_fault() {
        assert_eq!(action("steer").ok(), Some(DurableTaskInputAction::Steer));
        assert_eq!(
            action("refresh").ok(),
            Some(DurableTaskInputAction::Refresh)
        );
        for stored in ["rewrite", "", "Steer"] {
            let unknown = action(stored).expect_err("an unknown spelling is refused");
            assert_eq!(unknown.code(), error_code::INTERNAL_DB_QUERY, "{stored:?}");
        }
    }

    // Value: protects=a nullable stored counter stays absent when unset and is
    // refused when negative; fails_when=a negative generation or input sequence
    // wraps to a huge unsigned value or reads as absent; why_new=four readers
    // shared a copied map-and-transpose, now this one; seam=the row decoder.
    #[test]
    fn an_optional_counter_keeps_absence_and_refuses_a_negative_value() {
        assert_eq!(optional_counter(None, "generation").ok(), Some(None));
        assert_eq!(optional_counter(Some(0), "generation").ok(), Some(Some(0)));
        assert_eq!(
            optional_counter(Some(i64::MAX), "generation").ok(),
            Some(Some(i64::MAX.unsigned_abs()))
        );
        let negative = optional_counter(Some(-1), "input sequence")
            .expect_err("a negative stored counter is refused");
        assert_eq!(negative.code(), error_code::INTERNAL_DB_QUERY);
        assert!(
            negative.to_string().contains("input sequence"),
            "the refusal names its column: {negative}"
        );
    }
}
