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
