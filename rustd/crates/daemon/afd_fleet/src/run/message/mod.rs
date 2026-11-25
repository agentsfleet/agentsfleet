//! The interim message verb: one line said in the event's thread before the
//! answer, prepared here and posted by `afd_outbound`'s posters.
//!
//! ```text
//!   proved run ──► the event's thread ──► masked text
//!        │                  │                  │
//!   stale: refuse     none: no channel      Interim
//! ```
//!
//! Every refusal is decided before the text is read, so a refused message
//! costs no vault read. The run's standing is the verb's one fence check: it
//! is read without a lock and nothing is written to the run, so a message
//! never waits on a report, a renewal or a tool-call post of its run. The
//! line's marker part is the call's own number, so a retried call names the
//! line its first attempt may have posted. A run may say as much, and as
//! often, as it likes: no count is kept, the channel refuses what it cannot
//! take, and the post answers `delivered: false`.
//!
//! # What the mask covers
//!
//! The fleet's declared static credentials, opened from the vault the way the
//! run that runs it opened them, and masked with the runner's own `Scrub`.
//! A minted token is the broker's and is never held here; the runner masks
//! those before it sends, with the guard that minted them.

use af_core::id::Uuid7;
use af_secrets::{Scrub, StaticSecrets};
use af_wire::message_verb::MessageRequest;
use af_wire::turn::FleetSteerBody;
use afd_outbound::Interim;

use crate::error::{Result, message_no_channel, run_not_found};
use crate::run::installed::Installed;
use crate::run::pull::RunService;
use crate::run::standing::Standing;
use crate::run::store::Runs;

/// Who a fleet's steer is from, and the body it stores.
///
/// The body is composed and serialized here, once, from the masked words,
/// so the stored request, the live frame and the key all read the same text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteerSender {
    /// The text, with the sender fleet's declared static secrets masked.
    pub text: String,
    /// The sender line, a newline, then `text`: what the target reads.
    pub message: String,
    /// The stored body, `af_wire::turn::FleetSteerBody`, serialized.
    pub request_json: String,
}

impl RunService {
    /// One line for `run_id`'s thread, fenced and scrubbed, numbered by its
    /// call.
    ///
    /// # Errors
    /// Refuses a run that is not this runner's or not live, a superseded
    /// holder, and an event that recorded no thread. Reports a datastore or
    /// vault that would not answer, and a mask that could not be built — in
    /// which case nothing is posted.
    pub async fn message(
        &self,
        runner_id: &Uuid7,
        run_id: Uuid7,
        request: &MessageRequest<'_>,
    ) -> Result<Interim> {
        let standing = self
            .standing(runner_id, run_id, request.fencing_token)
            .await?;
        let reply = {
            let mut connection = self.runs.pool().acquire().await?;
            Runs::reply_destination(&mut connection, &standing.fleet_id, &standing.turn_id).await?
        }
        .ok_or_else(message_no_channel)?;
        let text = self.masked(&standing, &request.text).await?;
        let Standing {
            run_id,
            fleet_id,
            workspace_id,
            turn_id,
            ..
        } = standing;
        Ok(Interim {
            provider: reply.provider,
            destination: reply.address,
            workspace_id,
            fleet_id,
            turn_id,
            run_id,
            text,
            part: request.call_number,
        })
    }

    /// `text` with every declared static secret of the run's fleet masked.
    ///
    /// What a message posts and what a schedule a fleet writes stores, so a
    /// secret the model echoes reaches neither a thread nor the scheduler.
    ///
    /// # Errors
    /// Refuses a fleet that is no longer installed. Reports a datastore or
    /// vault that would not answer, and a mask that could not be built.
    pub async fn masked(&self, standing: &Standing, text: &str) -> Result<String> {
        self.installed_for(standing).await?;
        self.mask_with(standing, text).await
    }

    /// The sender of a fleet's steer, from one read of the installed fleet:
    /// its saved name, `text` masked as [`RunService::masked`] masks it, and
    /// the body the steer stores.
    ///
    /// # Errors
    /// As [`RunService::masked`]: a fleet no longer installed is refused as
    /// a run that is not live.
    pub async fn sender(&self, standing: &Standing, text: &str) -> Result<SteerSender> {
        let installed = self.installed_for(standing).await?;
        let text = self.mask_with(standing, text).await?;
        let body = FleetSteerBody::new(
            &text,
            standing.fleet_id.as_str(),
            &installed.name,
            standing.turn_id.as_str(),
        );
        let request_json = serde_json::to_string(&body).map_err(af_core::json::Unreadable::from)?;
        let message = body.message.into_owned();
        Ok(SteerSender {
            text,
            message,
            request_json,
        })
    }

    /// The run's fleet as installed, or the refusal a fleet that no longer
    /// runs earns.
    async fn installed_for(&self, standing: &Standing) -> Result<Installed> {
        self.runs
            .installed(&standing.fleet_id)
            .await?
            .ok_or_else(run_not_found)
    }

    /// `text` with the static secrets a person approved for the fleet masked.
    async fn mask_with(&self, standing: &Standing, text: &str) -> Result<String> {
        let approved = {
            let mut connection = self.runs.pool().acquire().await?;
            afd_gate::policy::approved::Approved::read_on(&mut connection, &standing.fleet_id)
                .await?
        };
        let names = approved.names();
        if names.is_empty() {
            return Ok(text.to_owned());
        }
        let declared = afd_credential::secrets::declared(
            &self.vault,
            &standing.workspace_id,
            &names,
            self.broker.connectors(),
        )
        .await?;
        let scrub = Scrub::of(StaticSecrets::of_map(declared.secrets_map()).values())?;
        Ok(scrub.text(text).into_owned())
    }
}
