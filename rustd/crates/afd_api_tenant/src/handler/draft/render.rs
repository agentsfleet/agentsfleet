//! Move validated domain projections into wire payloads without copying setup JSON.

use afd_api_wire::authoring as wire;
use afd_authoring::{Cancellation, types};
use afd_fleet_runtime::Definition;

pub(super) fn draft(value: types::Draft) -> wire::Draft<Definition, types::Review> {
    wire::Draft {
        id: value.id.to_string(),
        state: value.state.as_str().into(),
        title: value.title,
        etag: value.etag,
        authoring_model_ref: value.authoring_model_ref,
        cancelled_through: value.cancelled_through,
        candidate: value.candidate.map(candidate),
        review: value.review,
        linked_fleet_id: value.linked_fleet_id.map(|id| id.to_string()),
        publication: value.publication.map(|receipt| wire::Publication {
            library_id: receipt.library_id,
            revision_id: receipt.revision_id.to_string(),
            canonical_digest: receipt.canonical_digest,
        }),
        closed_at: value.closed_at,
        expires_at: value.expires_at,
    }
}

fn candidate(value: types::Candidate) -> wire::Candidate<Definition> {
    wire::Candidate {
        id: value.id.to_string(),
        revision: value.revision,
        definition: value.definition,
        canonical_digest: value.canonical_digest,
        policy_semantics: value.policy_semantics,
    }
}

pub(super) fn accepted(value: types::Accepted) -> wire::Accepted {
    wire::Accepted {
        status: value.status.into(),
        sequence: value.sequence,
        replayed: value.replayed,
    }
}

pub(super) fn card(value: types::DraftCard) -> wire::DraftCard {
    wire::DraftCard {
        id: value.id.to_string(),
        title: value.title,
        state: value.state.as_str().into(),
        etag: value.etag,
        linked_fleet_id: value.linked_fleet_id.map(|id| id.to_string()),
        updated_at: value.updated_at,
        expires_at: value.expires_at,
        candidate_revision: value.candidate_revision,
        setup_revision: value.setup_revision,
        publication_revision: value.publication_revision,
    }
}

pub(super) fn message(value: types::Message) -> wire::Message {
    wire::Message {
        sequence: value.sequence,
        operation_id: value.operation_id.to_string(),
        role: value.role.into(),
        state: value.state.as_str().into(),
        text: value.text,
        candidate_id: value.candidate_id.map(|id| id.to_string()),
        usage: value.usage,
        failure: value.failure.map(|failure| wire::Failure {
            reason: failure.reason,
            error_code: failure.error_code.to_string(),
            recovery_action: failure.recovery_action.into(),
            cause: failure.cause.map(|cause| cause.as_str().to_owned()),
        }),
        created_at: value.created_at,
        finished_at: value.finished_at,
    }
}

pub(super) fn page<T, U>(value: types::Page<T>, render: impl FnMut(T) -> U) -> wire::CursorPage<U> {
    wire::CursorPage {
        items: value.items.into_iter().map(render).collect(),
        total: value.total,
        next_cursor: value.next_cursor,
    }
}

pub(super) fn cancellation(value: Cancellation) -> wire::Cancellation {
    wire::Cancellation {
        cancelled_through: value.cancelled_through,
        stopped: value
            .stopped
            .into_iter()
            .map(|value| wire::Stopped {
                sequence: value.sequence,
                state: value.state.into(),
            })
            .collect(),
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "fixed test fixtures fail loudly when their contract changes"
)]
mod tests {
    use afd_core::id::Uuid7;

    use afd_wire::authoring::FailureReason;

    use super::{card, message, types};

    /// A card at `revisions`: candidate, saved setup and publication.
    fn listed(revisions: (Option<i64>, Option<i64>, Option<i64>)) -> types::DraftCard {
        types::DraftCard {
            id: Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010")
                .unwrap_or_else(|error| panic!("fixture id is canonical: {error}")),
            title: "Triage the inbox".to_owned(),
            state: types::DraftState::Editing,
            etag: "\"v1\"".to_owned(),
            linked_fleet_id: None,
            updated_at: jiff::Timestamp::constant(1_760_000_000, 0),
            expires_at: None,
            candidate_revision: revisions.0,
            setup_revision: revisions.1,
            publication_revision: revisions.2,
        }
    }

    #[test]
    fn a_card_lists_every_revision_summary_and_null_for_one_it_has_not_reached() {
        for (revisions, expected) in [
            ((Some(3), Some(2), Some(1)), [3.into(), 2.into(), 1.into()]),
            (
                (Some(1), None, None),
                [1.into(), serde_json::Value::Null, serde_json::Value::Null],
            ),
            (
                (None, None, None),
                [
                    serde_json::Value::Null,
                    serde_json::Value::Null,
                    serde_json::Value::Null,
                ],
            ),
        ] {
            let wire = serde_json::to_value(card(listed(revisions)))
                .unwrap_or_else(|error| panic!("a card serializes: {error}"));
            assert_eq!(
                [
                    wire["candidate_revision"].clone(),
                    wire["setup_revision"].clone(),
                    wire["publication_revision"].clone(),
                ],
                expected,
                "each summary is present on every card, null until the chat reaches it"
            );
        }
    }

    /// A failed message's recovery reaches the wire as the domain derived it:
    /// the reason, its registry code and the action a client renders.
    #[test]
    fn a_failed_message_carries_its_reason_code_and_recovery_onto_the_wire() {
        let failure = types::Failure::from(FailureReason::InsufficientBalance);
        let (code, action) = (failure.error_code, failure.recovery_action);
        let accepted = jiff::Timestamp::constant(1_760_000_000, 0);

        let rendered = message(types::Message {
            sequence: "7".to_owned(),
            operation_id: Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011")
                .unwrap_or_else(|error| panic!("fixture id is canonical: {error}")),
            role: "assistant",
            state: types::MessageState::Failed,
            text: None,
            candidate_id: None,
            usage: None,
            failure: Some(failure),
            created_at: accepted,
            finished_at: Some(accepted),
        });

        let wire = rendered
            .failure
            .unwrap_or_else(|| panic!("a failed message keeps its failure"));
        assert_eq!(wire.reason, FailureReason::InsufficientBalance);
        assert_eq!(wire.error_code, code.as_str());
        assert_eq!(wire.recovery_action, action);
    }
}
