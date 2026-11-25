//! A trigger add's kind, source and shape, decided without a datastore.

use std::borrow::Cow;

use af_core::error_code;
use af_wire::trigger_verb::{TriggerAddRequest, TriggerKind, TriggerVerifier};

use super::check;

fn add(kind: TriggerKind) -> TriggerAddRequest<'static> {
    TriggerAddRequest {
        fencing_token: 1,
        item_id: Cow::Borrowed("019a0000-0000-7000-8000-000000000001"),
        call_number: 1,
        kind,
        source: None,
        events: None,
        repositories: None,
        verifier: None,
        secret_name: None,
        channel: None,
    }
}

fn webhook(source: &'static str) -> TriggerAddRequest<'static> {
    TriggerAddRequest {
        source: Some(Cow::Borrowed(source)),
        verifier: Some(TriggerVerifier {
            scheme: Cow::Borrowed("svix-v1"),
            header: None,
        }),
        secret_name: Some(Cow::Borrowed("sentry-hook")),
        ..add(TriggerKind::Webhook)
    }
}

#[test]
fn each_kind_takes_its_one_source() {
    let accepted = [
        (webhook("sentry"), "sentry"),
        (add(TriggerKind::AppEvent), "github"),
        (
            TriggerAddRequest {
                repositories: Some(vec![Cow::Borrowed("org/app")]),
                ..add(TriggerKind::AppEvent)
            },
            "github",
        ),
        (
            TriggerAddRequest {
                channel: Some(Cow::Borrowed("C0123456789")),
                ..add(TriggerKind::Mention)
            },
            "slack",
        ),
    ];
    for (request, source) in accepted {
        let checked = check(&request);
        assert!(
            checked
                .as_ref()
                .is_ok_and(|checked| checked.source == source && checked.kind == request.kind),
            "{source}: {:?}",
            checked.err()
        );
    }
}

#[test]
fn a_reserved_missing_or_foreign_source_is_refused() {
    let refused = [
        webhook("approval"),
        webhook("github"),
        TriggerAddRequest {
            source: None,
            ..webhook("sentry")
        },
        TriggerAddRequest {
            source: Some(Cow::Borrowed("gitlab")),
            ..add(TriggerKind::AppEvent)
        },
        TriggerAddRequest {
            source: Some(Cow::Borrowed("discord")),
            channel: Some(Cow::Borrowed("C0123456789")),
            ..add(TriggerKind::Mention)
        },
    ];
    for request in refused {
        let code = check(&request).err().map(|refused| refused.code());
        assert_eq!(
            code,
            Some(error_code::INVALID_REQUEST),
            "{:?}",
            request.source
        );
    }
}

#[test]
fn a_shape_the_trigger_parser_refuses_is_refused() {
    let refused = [
        // No verifier, and no built-in scheme signs `sentry`.
        TriggerAddRequest {
            verifier: None,
            ..webhook("sentry")
        },
        // A direct message is not a channel.
        TriggerAddRequest {
            channel: Some(Cow::Borrowed("D0123456789")),
            ..add(TriggerKind::Mention)
        },
        // A mention names its channel.
        add(TriggerKind::Mention),
        TriggerAddRequest {
            repositories: Some(vec![Cow::Borrowed("not a repository")]),
            ..add(TriggerKind::AppEvent)
        },
    ];
    for request in refused {
        let code = check(&request).err().map(|refused| refused.code());
        assert_eq!(code, Some(error_code::INVALID_REQUEST), "{request:?}");
    }
}
