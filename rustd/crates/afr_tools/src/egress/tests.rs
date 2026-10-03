use afr_egress::Refusal;

use super::{answered, refused};
use crate::catalog::HTTP_REQUEST;
use crate::runtime::ToolErrorCode;

#[test]
fn should_read_each_refusal_under_its_own_code() {
    let cases = [
        (Refusal::HttpsRequired, ToolErrorCode::HttpsRequired),
        (
            Refusal::InvalidHeader {
                name: "x".to_owned(),
            },
            ToolErrorCode::InvalidArguments,
        ),
        (
            Refusal::HostNotAllowed {
                host: "h".to_owned(),
            },
            ToolErrorCode::HostNotAllowed,
        ),
        (
            Refusal::AddressNotAllowed {
                host: "h".to_owned(),
            },
            ToolErrorCode::AddressNotAllowed,
        ),
        (
            Refusal::CredentialMintRefused {
                detail: "d".to_owned(),
            },
            ToolErrorCode::CredentialMintRefused,
        ),
        (
            Refusal::UpstreamUnreachable {
                host: "h".to_owned(),
                reason: "r",
            },
            ToolErrorCode::UpstreamUnreachable,
        ),
    ];

    for (refusal, code) in cases {
        let output = refused(&HTTP_REQUEST, &refusal);
        assert_eq!(output.error_code, Some(code));
        assert_eq!(output.text, format!("[{code}] {refusal}"));
    }
}

#[test]
fn should_answer_2xx_as_succeeded_and_anything_else_as_upstream_status() {
    assert_eq!(answered(204, "Status: 204".to_owned()).error_code, None);
    for status in [199, 301, 404, 500] {
        assert_eq!(
            answered(status, String::new()).error_code,
            Some(ToolErrorCode::UpstreamStatus)
        );
    }
}
