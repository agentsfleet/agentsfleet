//! Trigger parsing contracts, including set-level uniqueness.

use super::{Cron, Trigger, WebhookSignature, parse_set, raw};
use crate::Error;
use crate::provider::StaticRegistry;

#[test]
fn cron_defaults_and_each_duplicate_class_are_deterministic() -> Result<(), Error> {
    let parsed = parse_set(
        vec![raw::Trigger::Cron {
            schedule: Some("0 * * * *".to_owned()),
            timezone: None,
            message: None,
        }],
        &StaticRegistry,
    )?;
    assert!(matches!(
        parsed.first(),
        Some(Trigger::Cron(Cron { schedule, timezone, message }))
            if schedule.as_ref() == "0 * * * *"
                && timezone.as_ref() == "UTC"
                && message.as_ref() == "Scheduled Fleet run"
    ));

    for duplicate in [
        vec![raw::Trigger::Api, raw::Trigger::Api],
        vec![cron("one"), cron("two")],
        vec![webhook("github", None), webhook("github", None)],
    ] {
        assert!(matches!(
            parse_set(duplicate, &StaticRegistry),
            Err(Error::InvalidTriggerSet { .. })
        ));
    }
    Ok(())
}

#[test]
fn webhook_signatures_use_provider_defaults_or_explicit_unknown_values() -> Result<(), Error> {
    let known = parse_set(
        vec![webhook(
            "slack",
            Some(raw::Signature {
                secret_ref: Some("slack_signing_secret".to_owned()),
                header: None,
                prefix: None,
                ts_header: None,
            }),
        )],
        &StaticRegistry,
    )?;
    let Some(Trigger::Webhook(known)) = known.first() else {
        return Err(Error::InvalidTriggerSet {
            reason: "the known webhook fixture changed shape",
        });
    };
    let Some(signature) = known.signature.as_ref() else {
        return Err(Error::InvalidTriggerSet {
            reason: "the known webhook fixture lost its signature",
        });
    };
    assert_signature(
        signature,
        "x-slack-signature",
        "v0=",
        Some("x-slack-request-timestamp"),
        "slack_signing_secret",
    );

    let explicit = parse_set(
        vec![webhook(
            "custom",
            Some(raw::Signature {
                secret_ref: Some("custom_secret".to_owned()),
                header: Some("x-custom-signature".to_owned()),
                prefix: Some("sig=".to_owned()),
                ts_header: Some("x-custom-time".to_owned()),
            }),
        )],
        &StaticRegistry,
    )?;
    let Some(Trigger::Webhook(explicit)) = explicit.first() else {
        return Err(Error::InvalidTriggerSet {
            reason: "the explicit webhook fixture changed shape",
        });
    };
    let Some(signature) = explicit.signature.as_ref() else {
        return Err(Error::InvalidTriggerSet {
            reason: "the explicit webhook fixture lost its signature",
        });
    };
    assert_signature(
        signature,
        "x-custom-signature",
        "sig=",
        Some("x-custom-time"),
        "custom_secret",
    );
    Ok(())
}

#[test]
fn arity_and_required_trigger_fields_fail_closed() {
    assert!(matches!(
        parse_set(Vec::new(), &StaticRegistry),
        Err(Error::InvalidTriggerSet { .. })
    ));
    assert!(matches!(
        parse_set(
            std::iter::repeat_with(|| raw::Trigger::Api)
                .take(9)
                .collect(),
            &StaticRegistry
        ),
        Err(Error::InvalidTriggerSet { .. })
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
            parse_set(vec![trigger], &StaticRegistry),
            Err(Error::InvalidTriggerSet { .. })
        ));
    }

    for signature in [
        raw::Signature {
            secret_ref: None,
            header: Some("x-signature".to_owned()),
            prefix: None,
            ts_header: None,
        },
        raw::Signature {
            secret_ref: Some("secret".to_owned()),
            header: None,
            prefix: None,
            ts_header: None,
        },
    ] {
        assert!(matches!(
            parse_set(vec![webhook("custom", Some(signature))], &StaticRegistry),
            Err(Error::InvalidSignatureConfig { .. })
        ));
    }
}

fn cron(message: &str) -> raw::Trigger {
    raw::Trigger::Cron {
        schedule: Some("0 * * * *".to_owned()),
        timezone: Some("Asia/Kolkata".to_owned()),
        message: Some(message.to_owned()),
    }
}

fn webhook(source: &str, signature: Option<raw::Signature>) -> raw::Trigger {
    raw::Trigger::Webhook {
        source: Some(source.to_owned()),
        events: Some(vec!["push".to_owned()]),
        repositories: Some(vec!["agentsfleet/agentsfleet".to_owned()]),
        credential_name: Some("github_app".to_owned()),
        signature,
    }
}

fn assert_signature(
    signature: &WebhookSignature,
    header: &str,
    prefix: &str,
    timestamp: Option<&str>,
    secret: &str,
) {
    assert_eq!(signature.header(), header);
    assert_eq!(signature.prefix(), prefix);
    assert_eq!(signature.timestamp_header(), timestamp);
    assert_eq!(signature.secret_ref(), secret);
}
