use std::borrow::Cow;

use afd_wire::runner::{LABEL_MAX_BYTES, LABELS_MAX};
use garde::Validate as _;

use super::{PlatformKeyPut, RunnerAdminPatchRequest, RunnerEventType};

#[test]
fn every_runner_history_value_serializes_as_the_spelling_postgres_stores() {
    for (value, expected) in [
        (RunnerEventType::RunnerRegistered, "runner_registered"),
        (RunnerEventType::RunnerOnline, "runner_online"),
        (RunnerEventType::RunnerOffline, "runner_offline"),
        (RunnerEventType::LeaseAcquired, "lease_acquired"),
        (RunnerEventType::LeaseReleased, "lease_released"),
        (RunnerEventType::RunnerCordoned, "runner_cordoned"),
        (RunnerEventType::RunnerDraining, "runner_draining"),
        (RunnerEventType::RunnerDrained, "runner_drained"),
        (RunnerEventType::RunnerRevoked, "runner_revoked"),
        (RunnerEventType::RunnerTokenRotated, "runner_token_rotated"),
        (
            RunnerEventType::RunnerPolicyAssigned,
            "runner_policy_assigned",
        ),
    ] {
        let encoded = serde_json::to_value(value);
        assert_eq!(
            encoded.as_ref().ok().and_then(serde_json::Value::as_str),
            Some(expected)
        );
    }
}

/// The runner patch refuses a key it does not know, so a misspelled `lables`
/// is never read as "no label change".
#[test]
fn a_runner_patch_refuses_an_unknown_label_key() {
    /// The decoded label edit, or `Err` when the body does not decode.
    fn labels(body: &str) -> Result<Option<Vec<String>>, ()> {
        serde_json::from_str::<RunnerAdminPatchRequest<'_>>(body)
            .map(|request| {
                request
                    .labels
                    .map(|labels| labels.into_iter().map(Cow::into_owned).collect())
            })
            .map_err(|_refused| ())
    }

    assert_eq!(
        labels(r#"{"lables":["gpu"]}"#),
        Err(()),
        "an unknown key must not decode"
    );
    assert_eq!(labels("{}"), Ok(None), "absent labels are no label change");
    assert_eq!(
        labels(r#"{"labels":[]}"#),
        Ok(Some(Vec::new())),
        "an empty list clears"
    );
}

/// The bounds the patch proves are the enrolment bounds, at their edge, and an
/// absent label set proves nothing at all.
#[test]
fn a_runner_patch_proves_the_enrolment_label_bounds() {
    let proved = |labels: Option<Vec<Cow<'static, str>>>| {
        RunnerAdminPatchRequest {
            action: None,
            assigned_policy: None,
            labels,
        }
        .validate()
        .is_ok()
    };

    assert_eq!(
        [
            proved(None),
            proved(Some(vec![Cow::Borrowed("gpu"); LABELS_MAX])),
            proved(Some(vec![Cow::Borrowed("gpu"); LABELS_MAX + 1])),
            proved(Some(vec![Cow::Owned("l".repeat(LABEL_MAX_BYTES))])),
            proved(Some(vec![Cow::Owned("l".repeat(LABEL_MAX_BYTES + 1))])),
        ],
        [true, true, false, true, false],
        "absent, at the count cap, past it, at the byte cap, past it"
    );
}

/// The field description publishes the two bounds as numbers, so a change to
/// either constant must change that sentence too. This is what fails first.
#[test]
fn the_published_label_bounds_match_the_constants() {
    assert_eq!((LABELS_MAX, LABEL_MAX_BYTES), (32, 64));
}

/// Postgres refuses NUL inside `text`, so a NUL in the provider or the model
/// is the caller's 400 rather than the store's 500.
#[test]
fn a_platform_key_put_refuses_a_nul_in_provider_or_model() {
    let put = |provider: &'static str, model: &'static str| PlatformKeyPut {
        provider: Cow::Borrowed(provider),
        source_workspace_id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d02"),
        model: Cow::Borrowed(model),
        base_url: None,
    };
    assert!(
        put("anthropic", "claude-opus-5").validate().is_ok(),
        "a clean body passes"
    );
    for broken in [
        put("anth\0ropic", "claude-opus-5"),
        put("anthropic", "claude\0-opus-5"),
    ] {
        let refused = broken.validate().err().map(|report| report.to_string());
        assert!(
            refused
                .as_deref()
                .is_some_and(|report| report.contains(afd_validate::rules::HAS_NUL)),
            "a NUL is refused as one: {refused:?}"
        );
    }
}
