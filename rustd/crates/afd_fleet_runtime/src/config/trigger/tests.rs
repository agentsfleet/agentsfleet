//! Trigger parsing and set-level uniqueness.

use super::{
    Cron, MAX_TRIGGERS, Trigger, VERIFIED_WEBHOOK_SOURCES, WebhookVerifier, parse_set, raw,
};
use crate::Error;
use crate::error::ErrorKind;

#[test]
fn cron_defaults_and_each_duplicate_class_are_deterministic() -> Result<(), Error> {
    let parsed = parse_set(vec![raw::Trigger::Cron {
        schedule: Some("0 * * * *".to_owned()),
        timezone: None,
        message: None,
    }])?;
    assert!(matches!(
        parsed.first(),
        Some(Trigger::Cron(Cron { schedule, timezone, message }))
            if schedule.as_ref() == "0 * * * *"
                && timezone.as_ref() == "UTC"
                && message.as_ref() == "Scheduled Fleet run"
    ));

    for duplicate in [
        vec![cron("one"), cron("two")],
        vec![webhook("github", None), webhook("github", None)],
    ] {
        assert!(matches!(
            parse_set(duplicate),
            Err(ref refusal) if matches!(refusal.kind(), ErrorKind::InvalidTriggerSet { .. })
        ));
    }
    Ok(())
}

/// The exported source names are exactly the scheme table's, in its order, so
/// the prompt and the refusal built from them cannot drift from the ingress.
#[test]
fn verified_sources_are_the_schemes() {
    let schemes: Vec<_> = afd_webhook::Scheme::ALL
        .iter()
        .map(|scheme| scheme.source())
        .collect();
    assert_eq!(VERIFIED_WEBHOOK_SOURCES.as_slice(), schemes.as_slice());
}

/// A source with a scheme this daemon ships needs nothing more; any other
/// source is verifiable only on the Svix route, so it must name its secret.
#[test]
fn a_webhook_is_valid_only_when_some_route_can_verify_it() -> Result<(), Error> {
    for source in VERIFIED_WEBHOOK_SOURCES {
        parse_set(vec![webhook(source, None)])?;
    }

    let svix = parse_set(vec![webhook("stripe", Some(raw::Verifier::SvixV1 {}))])?;
    assert!(matches!(
        svix.first(),
        Some(Trigger::Webhook(hook))
            if hook.verifier.as_ref().map(WebhookVerifier::family) == Some(afd_webhook::Family::SvixV1)
    ));

    assert!(matches!(
        parse_set(vec![webhook("stripe", None)]),
        Err(ref refused) if matches!(
            refused.kind(),
            ErrorKind::UnverifiableWebhookSource { provider } if &**provider == "stripe"
        )
    ));
    Ok(())
}

/// Source-addressed routing makes declaration order irrelevant.
#[test]
fn a_svix_signed_source_can_follow_another_webhook() -> Result<(), Error> {
    let parsed = parse_set(vec![
        webhook("github", None),
        webhook("clerk", Some(raw::Verifier::SvixV1 {})),
    ])?;
    assert_eq!(parsed.len(), 2);
    Ok(())
}

/// An empty set is a fleet only ever talked to: it parses, and declares
/// nothing that could wake it besides a message.
#[test]
fn an_empty_trigger_set_is_a_message_only_fleet() -> Result<(), Error> {
    assert!(parse_set(Vec::new())?.is_empty());
    Ok(())
}

#[test]
fn arity_and_required_trigger_fields_fail_closed() {
    // The count is proved before any entry is read, so nine copies of one
    // trigger are refused for their number, not their sameness.
    assert!(matches!(
        parse_set(
            std::iter::repeat_with(|| cron("hourly"))
                .take(MAX_TRIGGERS + 1)
                .collect(),
        ),
        Err(ref refusal) if matches!(
            refusal.kind(),
            ErrorKind::InvalidTriggerSet { reason } if *reason == super::set::REASON_SET_TOO_LARGE
        )
    ));
    for trigger in [
        webhook("", None),
        raw::Trigger::Cron {
            schedule: Some(String::new()),
            timezone: None,
            message: None,
        },
    ] {
        assert!(matches!(
            parse_set(vec![trigger]),
            Err(ref refusal) if matches!(refusal.kind(), ErrorKind::InvalidTriggerSet { .. })
        ));
    }

    assert!(matches!(
        parse_set(vec![webhook("stripe", Some(raw::Verifier::BodyHex { header: None }))]),
        Err(ref refusal) if matches!(refusal.kind(), ErrorKind::InvalidSignatureConfig { .. })
    ));
}

fn cron(message: &str) -> raw::Trigger {
    raw::Trigger::Cron {
        schedule: Some("0 * * * *".to_owned()),
        timezone: Some("Asia/Kolkata".to_owned()),
        message: Some(message.to_owned()),
    }
}

fn webhook(source: &str, verifier: Option<raw::Verifier>) -> raw::Trigger {
    raw::Trigger::Webhook {
        source: Some(source.to_owned()),
        events: Some(vec!["push".to_owned()]),
        repositories: Some(vec!["agentsfleet/agentsfleet".to_owned()]),
        credential_name: Some("github_app".to_owned()),
        verifier,
    }
}
