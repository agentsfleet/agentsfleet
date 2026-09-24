//! A routed mention becomes one admission, owed back to its thread.

use afd_admission::{Admission, Admitted, Key, Producer, Reply};
use afd_core::id::Uuid7;
use afd_wire::event::EventType;

use crate::Ingress;
use crate::error::Result;

/// One mention, as the ledger admits it.
///
/// A struct rather than eight positional `&str`s: a transposition between the
/// team and the event, or the address and the body, admits a wrong row and
/// compiles clean.
#[derive(Debug, Clone, Copy)]
pub struct MentionAdmission<'a> {
    /// The fleet routing chose.
    pub fleet: &'a Uuid7,
    /// The workspace the team resolved to.
    pub workspace: &'a Uuid7,
    /// The provider's team the mention arrived from.
    pub team_id: &'a str,
    /// The provider's own id for this delivery, which its retries repeat.
    pub event_id: &'a str,
    /// Who mentioned the bot.
    pub user: &'a str,
    /// The composed event body.
    pub request_json: &'a str,
    /// The connector that answers, as its registry spells it. `&'static` for
    /// the reason [`Reply::To`] gives.
    pub connector: &'static str,
    /// Where in the thread the answer goes.
    pub address: &'a str,
}

impl Ingress {
    /// Admits a routed mention, at most once however often the provider
    /// retries it.
    ///
    /// Keyed `<team_id>:<event_id>`: the provider signs the body the event id
    /// rides in and repeats it on every retry, so a retry answers the first
    /// admission and a forged repeat cannot vary the key without breaking the
    /// signature. The reply destination is the thread, so the answer is owed
    /// back to where the question was asked.
    ///
    /// # Errors
    /// Reports a database that would not record the acceptance.
    pub async fn admit_mention(&self, mention: MentionAdmission<'_>) -> Result<Admitted> {
        let MentionAdmission {
            fleet,
            workspace,
            team_id,
            event_id,
            user,
            request_json,
            connector,
            address,
        } = mention;
        let key = format!("{team_id}:{event_id}");
        let actor = format!("{connector}:{user}");
        Ok(self
            .admissions
            .admit(Admission {
                producer: Producer::SlackMention,
                key: Key::Repeated(&key),
                fleet: fleet.as_str(),
                workspace: workspace.as_str(),
                actor: &actor,
                event_type: EventType::Chat,
                request_json,
                reply: Reply::To { connector, address },
            })
            .await?)
    }
}
