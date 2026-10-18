//! Behavioural coverage for administration error boundaries.

use std::error::Error as _;

use afd_core::error_code;

use super::{Error, row};

#[test]
fn query_and_row_failures_keep_causes_behind_safe_details() -> Result<(), &'static str> {
    let query = super::query("listing models")(sqlx::Error::Protocol(
        "fixture administration query".into(),
    ));
    assert_error(&query, error_code::INTERNAL_DB_QUERY, "Database error");

    let malformed_id = afd_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("the malformed identifier fixture unexpectedly parsed")?;
    let malformed = row("model.platform_models", "id")(malformed_id);
    assert_error(&malformed, error_code::INTERNAL_DB_QUERY, "Database error");

    Ok(())
}

#[test]
fn an_unreachable_datastore_is_the_one_unavailable_failure() {
    let unreachable = Error::from(afd_db::error::invalid_bool_knob("FIXTURE_KNOB"));
    assert_eq!(unreachable.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    assert_eq!(
        unreachable.detail(),
        afd_core::error::DETAIL_DATABASE_UNAVAILABLE
    );
    assert!(unreachable.is_datastore_unavailable());
    assert!(unreachable.source().is_some());
}

#[cfg(feature = "test-util")]
#[test]
fn an_identifier_that_cannot_be_minted_is_internal_and_keeps_its_cause() -> Result<(), &'static str>
{
    let (entropy, control) = afd_crypto::entropy::Entropy::new_mocked();
    control.fail_next();
    let refused = entropy
        .uuid7(jiff::Timestamp::UNIX_EPOCH)
        .err()
        .ok_or("a failed draw must refuse the mint")?;
    assert_error(
        &Error::from(refused),
        error_code::INTERNAL_OPERATION_FAILED,
        "Internal operation failed",
    );
    Ok(())
}

// Value: protects=invalid saved inference settings map to actionable safe client errors with their cause;
// fails_when=endpoint, dialect or context errors become internal failures or expose configuration bytes;
// why_new=existing administration error tests cover query, row and identifier errors only;
// seam=none; no_claim=HTTP status mapping or database persistence (administration error boundary only).
#[test]
fn inference_errors_keep_safe_details_and_typed_causes() -> Result<(), &'static str> {
    use afd_wire::inference::{InferenceApi, ModelInference};

    for (base_url, dialect, output) in [
        (
            "https://private-provider-config@models.example.com/v1",
            None,
            16,
        ),
        (
            "https://models.example.com/v1",
            Some("private-provider-dialect"),
            16,
        ),
        ("https://models.example.com/v1", None, 65),
    ] {
        let settings = ModelInference {
            api: InferenceApi::Chat,
            base_url: base_url.into(),
            dialect: dialect.map(str::to_owned),
            max_output_tokens: output,
        };
        let cause = afd_inference::Configured::new(&settings, 64)
            .err()
            .ok_or("the invalid inference fixture unexpectedly validated")?;
        let error = Error::from(cause);
        assert_error(
            &error,
            error_code::INVALID_REQUEST,
            "Inference settings must specify a supported API, safe HTTPS base URL and output limit within the model context",
        );
        assert!(
            error
                .source()
                .and_then(|source| source.downcast_ref::<afd_inference::Error>())
                .is_some()
        );
        for rendered in [
            error.detail().to_owned(),
            error.to_string(),
            format!("{error:?}"),
        ] {
            assert!(
                !rendered.contains("private-provider"),
                "configuration leaked: {rendered}"
            );
        }
    }
    Ok(())
}

fn assert_error(error: &Error, code: afd_core::error_code::ErrorCode, detail: &'static str) {
    assert_eq!(error.code(), code);
    assert_eq!(error.detail(), detail);
    assert!(!error.is_datastore_unavailable());
    assert!(error.source().is_some());
    assert!(error.to_string().contains(code.as_str()));
}
