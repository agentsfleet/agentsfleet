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
