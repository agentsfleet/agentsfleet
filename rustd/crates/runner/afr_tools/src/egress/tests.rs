use af_egress::Refusal;
use af_egress::error::one_of_each_kind;
use guffbolt::ToolOutput;

use super::refused;
use crate::catalog::HTTP_REQUEST;
use crate::runtime::ToolErrorCode;

// Every refusal goes back to the model as an error: its code, then the
// sentence that names what was refused, never the registry's `[CODE]`.
#[test]
fn should_hand_every_refusal_back_as_an_error_under_its_own_code() {
    for (name, failure) in one_of_each_kind() {
        let code = failure
            .refusal()
            .map_or(ToolErrorCode::UpstreamUnreachable, ToolErrorCode::from);

        let output = ToolOutput::from(refused(HTTP_REQUEST.name(), "run-1", &failure));

        assert_eq!(output.error_code, Some(code.into()), "{name}");
        assert_eq!(
            output.text,
            format!("[{code}] {}", failure.detail()),
            "{name}"
        );
        assert!(!output.text.contains(failure.code().as_str()), "{name}");
    }
}

#[test]
fn should_map_every_refusal_to_its_code() {
    let cases = [
        (Refusal::InvalidUrl, ToolErrorCode::InvalidArguments),
        (Refusal::InvalidHeader, ToolErrorCode::InvalidArguments),
        (Refusal::HttpsRequired, ToolErrorCode::HttpsRequired),
        (Refusal::MethodNotAllowed, ToolErrorCode::MethodNotAllowed),
        (Refusal::HostNotAllowed, ToolErrorCode::HostNotAllowed),
        (Refusal::AddressNotAllowed, ToolErrorCode::AddressNotAllowed),
        (
            Refusal::PlacementNotAllowed,
            ToolErrorCode::CredentialPlacementNotAllowed,
        ),
        (
            Refusal::CredentialHostNotAllowed,
            ToolErrorCode::CredentialHostNotAllowed,
        ),
        (Refusal::SecretNotFound, ToolErrorCode::SecretNotFound),
        (
            Refusal::RequestPolicyNotAllowed,
            ToolErrorCode::RequestPolicyNotAllowed,
        ),
        (
            Refusal::CredentialMintRefused,
            ToolErrorCode::CredentialMintRefused,
        ),
        (
            Refusal::UpstreamUnreachable,
            ToolErrorCode::UpstreamUnreachable,
        ),
    ];

    for (refusal, code) in cases {
        assert_eq!(ToolErrorCode::from(refusal), code, "{refusal:?}");
    }
}
