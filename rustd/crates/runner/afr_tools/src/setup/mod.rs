//! `propose_setup`: a draft's setup as a `guffbolt` proposal document.
//!
//! ```text
//!  model ─► propose_setup ─► Propose<FleetSetup> ─► FleetSetup::apply(kept or head, arguments)
//!            read: settle, resolve anchors, name the head ─► Save asked? refuse
//!            ─► apply: the edit on the kept setup, or a first setup within its bound
//!            refused ─► "[kind] sentence" the model corrects in the same run
//!  report ◄─ Propose::kept() ─► Kept { base: Base, document: Definition }
//! ```
//!
//! The rules are `af_fleet_runtime::setup`'s, which the daemon checks a
//! reported setup with again, so what the model is told here is what the
//! daemon decides. A later edit applies on top of the setup the run kept and
//! names the same head, which is what the report's base names.

use af_fleet_runtime::Definition;
use af_fleet_runtime::setup::{self, Head, Refused, Rejection};
use guffbolt::tools::propose::{Document, Refusal};
use guffbolt::{ErrorCode, ToolSpec};
use serde_json::Value;

/// The event every proposal's outcome is logged under.
const EVENT_OUTCOME: &str = "setup_proposal_outcome";

/// The candidate a draft's setup turn starts from.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Its identifier, which an edit names.
    pub id: String,
    /// Its revision, which an edit names.
    pub revision: u64,
    /// Its setup.
    pub definition: Definition,
}

/// The candidate a kept setup was proposed against, which a report's base
/// names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    /// The candidate's identifier; `None` for a first setup.
    pub candidate_id: Option<String>,
    /// The candidate's revision; zero for a first setup.
    pub revision: u64,
}

/// A draft's setup, as one setup turn proposes changes to it.
#[derive(Debug)]
pub struct FleetSetup {
    candidate: Option<Candidate>,
    said: Option<String>,
    save_requested: bool,
    parameters: Value,
}

impl FleetSetup {
    /// The setup one turn proposes changes to.
    ///
    /// `candidate` is the draft's head, absent before its first setup; `said`
    /// is the person's message, whose wording an inserted instruction keeps;
    /// when `save_requested`, the message asks for no change and every
    /// proposal is refused.
    #[must_use]
    pub fn new(candidate: Option<Candidate>, said: Option<String>, save_requested: bool) -> Self {
        let parameters = setup::parameters(candidate.is_some());
        Self {
            candidate,
            said,
            save_requested,
            parameters,
        }
    }

    /// The setup `proposal` leaves on `kept`, the setup the run last kept, or
    /// on the head candidate before the run kept one.
    fn check(&self, kept: Option<&Definition>, proposal: &Value) -> Result<Definition, Refused> {
        let definition = kept.or_else(|| self.candidate.as_ref().map(|head| &head.definition));
        let revision = self
            .candidate
            .as_ref()
            .map(|head| head.revision.to_string());
        let head = self
            .candidate
            .as_ref()
            .zip(revision.as_deref())
            .zip(definition)
            .map(|((candidate, revision), definition)| Head {
                id: &candidate.id,
                revision,
                definition,
            });
        let native = setup::read(proposal.clone(), head.as_ref(), self.said.as_deref())?;
        if self.save_requested {
            return Err(Refused {
                reason: Rejection::EditNotRequested,
                detail: None,
            });
        }
        native.apply(definition)
    }

    /// What the model reads for `refused`: the checker's sentence when it
    /// named a rule, the head it must name when its edit named another, and
    /// the fixed correction otherwise.
    fn refusal(&self, refused: &Refused) -> Refusal {
        let reason = match (&refused.detail, refused.reason, &self.candidate) {
            (Some(failure), _, _) => setup::invalid_setup(&failure.told()),
            (None, Rejection::StaleCandidate, Some(head)) => {
                setup::stale_base(&head.id, head.revision)
            }
            (None, rejection, _) => rejection.feedback().to_owned(),
        };
        Refusal::new(ErrorCode::new(refused.reason.kind()), reason)
    }
}

impl Document for FleetSetup {
    type Accepted = Definition;
    type Revision = Base;

    fn schema(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: setup::NAME,
            description: setup::DESCRIPTION,
            parameters: &self.parameters,
        }
    }

    fn base(&self) -> Base {
        self.candidate.as_ref().map_or(
            Base {
                candidate_id: None,
                revision: 0,
            },
            |head| Base {
                candidate_id: Some(head.id.clone()),
                revision: head.revision,
            },
        )
    }

    fn apply(&self, head: Option<&Definition>, proposal: &Value) -> Result<Definition, Refusal> {
        self.check(head, proposal)
            .inspect(|_accepted| tracing::info!(event = EVENT_OUTCOME, outcome = "accepted"))
            .map_err(|refused| {
                // The field is the checker's own key; its sentence can name
                // authored text, so only the model hears that.
                let refusal = refused.reason.kind();
                let field = refused
                    .detail
                    .as_ref()
                    .and_then(af_fleet_runtime::Error::field);
                tracing::info!(event = EVENT_OUTCOME, outcome = "refused", refusal, field);
                self.refusal(&refused)
            })
    }
}

#[cfg(test)]
mod tests;
