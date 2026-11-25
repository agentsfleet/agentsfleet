#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use af_wire::policy::HttpEgress;
use garde::Validate as _;

use super::{CreateFleetRequest, PatchFleetRequest, PatchStatus};

const OPERATION: &str = "019a0000-0000-7000-8000-000000000001";

fn create(body: &str) -> Result<CreateFleetRequest, serde_json::Error> {
    serde_json::from_str(body)
}

#[test]
fn a_create_needs_only_a_message_and_an_operation() {
    let request = create(&format!(
        r#"{{"message": "watch checkout errors", "operation_id": "{OPERATION}"}}"#
    ))
    .expect("a bare create");

    request.validate().expect("a bare create holds its bounds");
    assert!(request.model_ref.is_none() && request.name.is_none());
}

#[test]
fn the_old_create_body_is_refused_as_an_unknown_field() {
    for body in [
        r#"{"source": {"kind": "draft", "draft_candidate_id": "x"}, "name": "a", "review_digest": "d", "start": {"kind": "none"}}"#.to_owned(),
        format!(r#"{{"message": "m", "operation_id": "{OPERATION}", "review_digest": "d"}}"#),
        format!(r#"{{"message": "m", "operation_id": "{OPERATION}", "start": {{"kind": "none"}}}}"#),
    ] {
        assert!(create(&body).is_err(), "{body}");
    }
}

#[test]
fn a_create_outside_its_bounds_is_refused() {
    let past = "m".repeat(af_validate::TEXT_MAX_CHARS + 1);
    for body in [
        format!(r#"{{"message": "", "operation_id": "{OPERATION}"}}"#),
        format!(r#"{{"message": "a\u0000b", "operation_id": "{OPERATION}"}}"#),
        format!(r#"{{"message": "{past}", "operation_id": "{OPERATION}"}}"#),
        r#"{"message": "m", "operation_id": "not-a-uuid"}"#.to_owned(),
        r#"{"message": "m", "operation_id": "019a0000-0000-4000-8000-000000000001"}"#.to_owned(),
        format!(r#"{{"message": "m", "operation_id": "{OPERATION}", "name": ""}}"#),
        format!(
            r#"{{"message": "m", "operation_id": "{OPERATION}", "name": "{}"}}"#,
            "n".repeat(65)
        ),
        format!(r#"{{"message": "m", "operation_id": "{OPERATION}", "model_ref": ""}}"#),
    ] {
        let request = create(&body).expect("the shape decodes");
        assert!(
            request.validate().is_err(),
            "{}",
            &body[..body.len().min(80)]
        );
    }
}

#[test]
fn an_explicit_null_is_refused_rather_than_read_as_absent() {
    for body in [
        format!(r#"{{"message": "m", "operation_id": "{OPERATION}", "model_ref": null}}"#),
        format!(r#"{{"message": "m", "operation_id": "{OPERATION}", "name": null}}"#),
        r#"{"http_egress": null}"#.to_owned(),
    ] {
        assert!(
            create(&body).is_err() && serde_json::from_str::<PatchFleetRequest>(&body).is_err(),
            "{body}"
        );
    }
}

#[test]
fn a_patch_reads_the_model_and_both_network_toggles() {
    let patch: PatchFleetRequest = serde_json::from_str(
        r#"{"model_ref": "platform:kimi", "http_egress": "public", "package_domains": true}"#,
    )
    .expect("a patch");

    patch.validate().expect("a patch holds its bounds");
    assert_eq!(patch.http_egress, Some(HttpEgress::Public));
    assert_eq!(patch.package_domains, Some(true));
    assert!(patch.opens_network());

    let rename: PatchFleetRequest =
        serde_json::from_str(r#"{"name": "support-triage"}"#).expect("a rename");
    assert!(!rename.opens_network());
}

#[test]
fn a_patch_may_only_archive() {
    let archive: PatchFleetRequest =
        serde_json::from_str(r#"{"status": "archived"}"#).expect("archive");
    assert_eq!(archive.status, Some(PatchStatus::Archived));
    assert_eq!(PatchStatus::Archived.as_str(), "archived");

    for body in [
        r#"{"status": "active"}"#,
        r#"{"status": "installing"}"#,
        r#"{"http_egress": "allow_all"}"#,
        r#"{"read_only": true}"#,
    ] {
        assert!(
            serde_json::from_str::<PatchFleetRequest>(body).is_err(),
            "{body}"
        );
    }
}
