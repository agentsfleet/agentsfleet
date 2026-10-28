//! A shared vault name must satisfy every runtime use the candidate declares.

use afd_credential::{secrets::connector::SLACK_INTEGRATION, vault::CredentialInspection};
use afd_fleet_runtime::config::Trigger;
use std::collections::BTreeMap;

use crate::types::{Candidate, Requirements};

pub(super) enum Purpose<'a> {
    Execution,
    Mention(&'a str),
    // No source: the parser accepts an unsigned webhook trigger only for a
    // source whose scheme the ingress ships, so there is nothing left to ask.
    Webhook,
    Svix,
}

pub(super) fn collect<'a>(
    candidate: &'a Candidate,
    requirements: &'a Requirements,
) -> BTreeMap<&'a str, Vec<Purpose<'a>>> {
    let mut needs: BTreeMap<&str, Vec<Purpose<'_>>> = BTreeMap::new();
    for name in &requirements.credentials {
        needs.entry(name).or_default().push(Purpose::Execution);
    }
    for trigger in candidate.definition.runtime().triggers() {
        let (name, purpose) = match trigger {
            Trigger::Mention(mention) => (&*mention.source, Purpose::Mention(&mention.source)),
            Trigger::Webhook(hook) => (
                hook.credential_name.as_deref().unwrap_or(&hook.source),
                if hook.verifier.as_ref().is_some_and(|v| {
                    matches!(v, afd_fleet_runtime::config::WebhookVerifier::SvixV1)
                }) {
                    Purpose::Svix
                } else {
                    Purpose::Webhook
                },
            ),
            Trigger::Cron(_) => continue,
        };
        needs.entry(name).or_default().push(purpose);
    }
    needs
}

pub(super) fn usable(purposes: &[Purpose<'_>], held: &CredentialInspection) -> bool {
    purposes.iter().all(|purpose| match purpose {
        Purpose::Execution => held.usable,
        Purpose::Mention(source) => {
            *source == SLACK_INTEGRATION
                && held.integration.as_deref() == Some(*source)
                && held.usable
        }
        Purpose::Webhook => held.webhook_usable,
        Purpose::Svix => held.svix_usable,
    })
}

pub(super) fn needs_integration(purposes: &[Purpose<'_>]) -> bool {
    purposes
        .iter()
        .any(|purpose| matches!(purpose, Purpose::Execution | Purpose::Mention(_)))
}
