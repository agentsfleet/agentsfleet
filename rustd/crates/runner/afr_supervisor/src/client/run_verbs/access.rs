//! The triggers and access verbs: what may wake the held fleet, and a secret
//! or integration it asks a person to approve.
//!
//! Split from [`super`] so each file reads one family of a run's verbs.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_wire::access_verb::AccessRequest;
use af_wire::paths;
use af_wire::trigger_verb::TriggerAddRequest;
use afr_tools::verbs::TriggerCall;

use super::{Body, ControlPlane, Page, Verb, fenced, member, minted, run_path};
use crate::error::Result;

impl ControlPlane {
    /// Sends one triggers call for the fleet `run_id` runs, fenced by
    /// `fencing_token`.
    ///
    /// # Errors
    /// A refusal, or a transport failure. An add names its call's item and is
    /// retried as a blip allows, so a retry answers the first trigger; a read
    /// or a removal is one attempt, and the model decides whether to try again.
    pub(crate) async fn triggers(
        &self,
        run_id: &Uuid7,
        fencing_token: u64,
        call: TriggerCall<'_>,
    ) -> Result<Body> {
        let collection = run_path(run_id, paths::RUN_TRIGGERS_SUFFIX);
        match call {
            TriggerCall::Add {
                call_number,
                trigger,
            } => {
                let item_id = minted()?;
                let body = TriggerAddRequest {
                    fencing_token,
                    item_id: Cow::Borrowed(item_id.as_str()),
                    call_number,
                    kind: trigger.kind,
                    source: trigger.source,
                    events: trigger.events,
                    repositories: trigger.repositories,
                    verifier: trigger.verifier,
                    secret_name: trigger.secret_name,
                    channel: trigger.channel,
                };
                self.once(Verb::TriggerAdd, collection, &body).await
            }
            TriggerCall::List => {
                let path = fenced(&collection, fencing_token, Page::default());
                self.send(Verb::TriggerList, path, None).await
            }
            TriggerCall::Remove { trigger } => {
                let path = member(&collection, trigger, None);
                let path = fenced(&path, fencing_token, Page::default());
                self.send(Verb::TriggerRemove, path, None).await
            }
        }
    }

    /// Asks a person to approve `request`'s name for the fleet `run_id` runs,
    /// fenced by `fencing_token` whatever fence `request` names.
    ///
    /// # Errors
    /// A refusal, or a transport failure. One attempt: a retry after a lost
    /// 201 would find the card it raised still pending and answer 200, so the
    /// run would go on rather than wait for the person.
    pub(crate) async fn access(
        &self,
        run_id: &Uuid7,
        fencing_token: u64,
        request: AccessRequest<'_>,
    ) -> Result<Body> {
        let body = AccessRequest {
            fencing_token,
            ..request
        };
        let path = run_path(run_id, paths::RUN_ACCESS_SUFFIX);
        self.send_json(Verb::Access, path, &body).await
    }
}

#[cfg(test)]
#[path = "access_tests.rs"]
mod tests;
