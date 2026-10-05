use super::{ToolErrorCode, ToolOutput};

#[test]
fn should_spell_each_error_code_as_the_thread_reads_it() {
    let spelled = [
        (ToolErrorCode::NotOffered, "tool_not_offered"),
        (
            ToolErrorCode::HostedToolUnavailable,
            "hosted_tool_unavailable",
        ),
        (ToolErrorCode::SandboxUnavailable, "sandbox_unavailable"),
        (ToolErrorCode::OutputLimitReached, "output_limit_reached"),
        (ToolErrorCode::InvalidArguments, "invalid_arguments"),
        (ToolErrorCode::MemoryFull, "memory_full"),
        (ToolErrorCode::HttpsRequired, "https_required"),
        (ToolErrorCode::MethodNotAllowed, "method_not_allowed"),
        (ToolErrorCode::HostNotAllowed, "host_not_allowed"),
        (ToolErrorCode::AddressNotAllowed, "address_not_allowed"),
        (
            ToolErrorCode::CredentialPlacementNotAllowed,
            "credential_placement_not_allowed",
        ),
        (
            ToolErrorCode::CredentialHostNotAllowed,
            "credential_host_not_allowed",
        ),
        (ToolErrorCode::SecretNotFound, "secret_not_found"),
        (
            ToolErrorCode::RequestPolicyNotAllowed,
            "request_policy_not_allowed",
        ),
        (
            ToolErrorCode::CredentialMintRefused,
            "credential_mint_refused",
        ),
        (ToolErrorCode::UpstreamUnreachable, "upstream_unreachable"),
        (ToolErrorCode::UpstreamStatus, "upstream_status"),
        (
            ToolErrorCode::WorkspaceMemoryNotGranted,
            "workspace_memory_not_granted",
        ),
        (ToolErrorCode::TimedOut, "timed_out"),
        (ToolErrorCode::Interrupted, "interrupted"),
        (ToolErrorCode::SessionCapReached, "session_cap_reached"),
        (ToolErrorCode::SessionNotFound, "session_not_found"),
        (
            ToolErrorCode::SubcommandNotAllowed,
            "subcommand_not_allowed",
        ),
        (ToolErrorCode::PathNotAllowed, "path_not_allowed"),
        (ToolErrorCode::FileNotFound, "file_not_found"),
        (ToolErrorCode::FileTooLarge, "file_too_large"),
        (ToolErrorCode::TextNotFound, "text_not_found"),
        (ToolErrorCode::HashMismatch, "hash_mismatch"),
        (ToolErrorCode::PatchInvalid, "patch_invalid"),
        (ToolErrorCode::NotAnImage, "not_an_image"),
        (
            ToolErrorCode::ImageInputUnavailable,
            "image_input_unavailable",
        ),
    ];

    for (code, spelling) in spelled {
        assert_eq!(code.as_str(), spelling);
        assert_eq!(code.to_string(), spelling);
    }
}

#[test]
fn should_lead_a_failed_output_with_its_code() {
    let output = ToolOutput::failed(ToolErrorCode::NotOffered, "shell is not offered");

    assert_eq!(output.text, "[tool_not_offered] shell is not offered");
    assert_eq!(output.error_code, Some(ToolErrorCode::NotOffered));
    assert_eq!(output.exit_code, None);
}

#[test]
fn should_carry_no_code_on_a_succeeded_output() {
    let output = ToolOutput::succeeded("42");

    assert_eq!(output.text, "42");
    assert_eq!(output.error_code, None);
    assert_eq!(output.exit_code, None);
}
