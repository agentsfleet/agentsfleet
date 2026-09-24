//! What each arm of [`HarnessIngress`] answers across the ingress seam.
//!
//! Split from the types beside it: those say what a test can arrange, and this
//! is how an arrangement becomes the answer a route acts on — the production
//! store's call in one arm, the script's recorded reply in the other. A child
//! module, so it reads [`Scripted`]'s fields without widening them.

use afd_api::services::WebhookIngress;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::secret::SecretBytes;
use afd_ingress::slack::{ChannelId, FleetName, MentionAdmission, Subscriber};
use afd_ingress::{Admitted, Binding, Delivery, Fanout, Result as IngressResult, Surface};

use super::{ENTRY_ID_SEQUENCE, HarnessIngress, Recorded, Scripted};

impl WebhookIngress for HarnessIngress {
    async fn binding(&self, fleet: &Uuid7, source: Option<&str>) -> IngressResult<Option<Binding>> {
        match self {
            Self::Unreachable(ingress) => ingress.binding(fleet, source).await,
            Self::Scripted(scripted) => {
                scripted
                    .asked
                    .lock()
                    .expect("no test holds this lock across a panic")
                    .push(source.map(str::to_owned));
                Ok(scripted.binding.clone())
            }
        }
    }

    async fn signing_secret(&self, binding: &Binding) -> IngressResult<Option<SecretBytes>> {
        match self {
            Self::Unreachable(ingress) => ingress.signing_secret(binding).await,
            Self::Scripted(scripted) => Ok(scripted.secret.clone()),
        }
    }

    async fn svix_secret(&self, binding: &Binding) -> IngressResult<Option<SecretBytes>> {
        match self {
            Self::Unreachable(ingress) => ingress.svix_secret(binding).await,
            Self::Scripted(scripted) => Ok(scripted.svix.clone()),
        }
    }

    async fn platform_secret(
        &self,
        admin_workspace: &Uuid7,
        key: &str,
    ) -> IngressResult<Option<SecretBytes>> {
        match self {
            Self::Unreachable(ingress) => ingress.platform_secret(admin_workspace, key).await,
            Self::Scripted(scripted) => Ok(scripted.platform_secret.clone()),
        }
    }

    async fn installation_workspace(
        &self,
        provider: &str,
        installation: &str,
    ) -> IngressResult<Option<Uuid7>> {
        match self {
            Self::Unreachable(ingress) => {
                ingress.installation_workspace(provider, installation).await
            }
            Self::Scripted(scripted) => Ok(scripted.installation.clone()),
        }
    }

    async fn subscribers(
        &self,
        workspace: &Uuid7,
        provider: &str,
        repository: &str,
        event: &str,
    ) -> IngressResult<Fanout> {
        match self {
            Self::Unreachable(ingress) => {
                ingress
                    .subscribers(workspace, provider, repository, event)
                    .await
            }
            Self::Scripted(scripted) => Ok(scripted.fanout()),
        }
    }

    async fn deliver(
        &self,
        surface: Surface,
        binding: &Binding,
        delivery: &Delivery<'_>,
    ) -> IngressResult<Admitted> {
        match self {
            Self::Unreachable(ingress) => ingress.deliver(surface, binding, delivery).await,
            Self::Scripted(scripted) => Ok(scripted.append(surface, binding, delivery)),
        }
    }

    async fn mention_subscribers(
        &self,
        workspace: &Uuid7,
        provider: &str,
        channel: &ChannelId,
    ) -> IngressResult<Vec<Subscriber>> {
        match self {
            Self::Unreachable(ingress) => {
                ingress
                    .mention_subscribers(workspace, provider, channel)
                    .await
            }
            // A script arranges no chat channel: every mention it sees is one
            // nobody attached a fleet to. The live suite owns routing.
            Self::Scripted(_) => Ok(Vec::new()),
        }
    }

    async fn admit_mention(&self, mention: MentionAdmission<'_>) -> IngressResult<Admitted> {
        match self {
            Self::Unreachable(ingress) => ingress.admit_mention(mention).await,
            Self::Scripted(scripted) => {
                Ok(scripted.claim(&format!("{}:{}", mention.team_id, mention.event_id)))
            }
        }
    }

    // A script arranges no chat channel, so no resident is bound, a binding
    // answers the fleet it was handed, and no fleet is found by name. The live
    // suite owns materialisation.
    async fn resident(
        &self,
        provider: &str,
        team: &str,
        channel: &ChannelId,
    ) -> IngressResult<Option<Uuid7>> {
        match self {
            Self::Unreachable(ingress) => ingress.resident(provider, team, channel).await,
            Self::Scripted(_) => Ok(None),
        }
    }

    async fn bind_resident(
        &self,
        provider: &str,
        team: &str,
        channel: &ChannelId,
        fleet: &Uuid7,
        now: UnixMillis,
    ) -> IngressResult<Uuid7> {
        match self {
            Self::Unreachable(ingress) => {
                ingress
                    .bind_resident(provider, team, channel, fleet, now)
                    .await
            }
            Self::Scripted(_) => Ok(fleet.clone()),
        }
    }

    async fn fleet_named(
        &self,
        workspace: &Uuid7,
        name: &FleetName,
    ) -> IngressResult<Option<Uuid7>> {
        match self {
            Self::Unreachable(ingress) => ingress.fleet_named(workspace, name).await,
            Self::Scripted(_) => Ok(None),
        }
    }
}

impl Scripted {
    /// The three answers a route acts on, from what this store was told.
    ///
    /// The ceiling wins over the subscriber list when both are arranged, which
    /// is the only order that makes sense: a test setting a matched count is
    /// asking for the refusal, and a list beside it is the fleets that count
    /// stands for.
    fn fanout(&self) -> Fanout {
        match (self.over_ceiling, self.subscribers.is_empty()) {
            (Some(count), _) => Fanout::TooMany(count),
            (None, true) => Fanout::Nobody,
            (None, false) => Fanout::To(self.subscribers.clone()),
        }
    }

    /// Records one append and answers it, repeating the first id for a repeat.
    ///
    /// The claim key is [`afd_ingress`]'s own — `{fleet}:{provider event id}` —
    /// composed here rather than imported because the production one is built
    /// inside `deliver` and never crosses a seam. That duplication is the
    /// reason this cannot stand in for the script: the two could drift, and
    /// only the integration lane would notice.
    fn append(&self, surface: Surface, binding: &Binding, delivery: &Delivery<'_>) -> Admitted {
        let fleet = binding.fleet().as_str().to_owned();
        let key = format!("{fleet}:{}", delivery.event_id);

        let mut log = self
            .log
            .lock()
            .expect("no test holds this lock across a panic");
        let mut claimed = self
            .claimed
            .lock()
            .expect("no test holds this lock across a panic");

        log.push(Recorded {
            surface,
            fleet,
            event_id: delivery.event_id.to_owned(),
            actor: delivery.actor.to_owned(),
            request_json: delivery.request_json.to_owned(),
        });

        let next = format!("{}{ENTRY_ID_SEQUENCE}", log.len());
        let replayed = claimed.contains_key(&key);
        let id = claimed.entry(key).or_insert(next).clone();

        // The ledger answers a LOGICAL id, so the stub does too: a receipt is
        // what the append returns and is not what a route renders back.
        Admitted { id, replayed }
    }

    /// Answers one claim key, repeating its first id for a repeat — the
    /// ledger's rule, for a mention the script admits without a binding.
    fn claim(&self, key: &str) -> Admitted {
        let mut claimed = self
            .claimed
            .lock()
            .expect("no test holds this lock across a panic");
        let next = format!("{}{ENTRY_ID_SEQUENCE}", claimed.len() + 1);
        let replayed = claimed.contains_key(key);
        let id = claimed.entry(key.to_owned()).or_insert(next).clone();
        Admitted { id, replayed }
    }
}
