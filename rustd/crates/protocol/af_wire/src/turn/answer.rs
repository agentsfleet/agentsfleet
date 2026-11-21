//! A person's answer to the question a run asked: the body of the turn that
//! continues the run.
//!
//! The asking run ended `waiting` with a checkpoint at the asking call. The
//! answer lands as a continuation turn carrying this body, and that turn's
//! run resumes from the asking turn's checkpoint with the answer as the
//! asking call's result.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::{TurnEnvelope, TurnKind};

/// The body of a continuation turn that answers a run's question.
///
/// The answer rides in `message`, so every reader of a turn's message
/// ([`super::message_of`]) shows it as what the person said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionAnswer<'a> {
    /// What the person answered.
    #[serde(borrow)]
    pub message: Cow<'a, str>,
    /// The turn whose run asked, and whose checkpoint the answering run
    /// resumes from.
    #[serde(borrow)]
    pub asked_turn_id: Cow<'a, str>,
}

impl TurnEnvelope<'_> {
    /// The answer this turn carries, when it continues a run that asked a
    /// question.
    #[must_use]
    pub fn question_answer(&self) -> Option<QuestionAnswer<'_>> {
        if self.event_type != TurnKind::Continuation {
            return None;
        }
        serde_json::from_str(&self.request_json).ok()
    }

    /// The turn whose checkpoint this turn's runs save and restore: the
    /// asking turn for an answer, and the turn itself otherwise.
    #[must_use]
    pub fn checkpoint_turn_id(&self) -> Cow<'_, str> {
        self.question_answer().map_or_else(
            || Cow::Borrowed(self.turn_id.as_ref()),
            |answer| Cow::Owned(answer.asked_turn_id.into_owned()),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::QuestionAnswer;
    use crate::turn::{TurnEnvelope, TurnKind};

    fn envelope(event_type: TurnKind, request_json: &str) -> TurnEnvelope<'_> {
        TurnEnvelope {
            turn_id: Cow::Borrowed("answering"),
            fleet_id: Cow::Borrowed("fleet"),
            workspace_id: Cow::Borrowed("workspace"),
            actor: Cow::Borrowed("continuation:asking"),
            event_type,
            request_json: Cow::Borrowed(request_json),
            created_at: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    #[test]
    fn an_answering_continuation_resumes_the_asking_turns_checkpoint() {
        let body = serde_json::to_string(&QuestionAnswer {
            message: Cow::Borrowed("yes"),
            asked_turn_id: Cow::Borrowed("asking"),
        })
        .unwrap_or_default();
        let turn = envelope(TurnKind::Continuation, &body);

        let answer = turn.question_answer();
        assert_eq!(answer.as_ref().map(|a| a.message.as_ref()), Some("yes"));
        assert_eq!(turn.checkpoint_turn_id(), "asking");
        assert_eq!(crate::turn::message_of(&body), "yes");
    }

    #[test]
    fn any_other_turn_keeps_its_own_checkpoint() {
        for (kind, body) in [
            (TurnKind::Continuation, "{}"),
            (TurnKind::Chat, r#"{"message":"yes","asked_turn_id":"asking"}"#),
        ] {
            let turn = envelope(kind, body);
            assert_eq!(turn.question_answer(), None, "{kind:?}");
            assert_eq!(turn.checkpoint_turn_id(), "answering", "{kind:?}");
        }
    }
}
