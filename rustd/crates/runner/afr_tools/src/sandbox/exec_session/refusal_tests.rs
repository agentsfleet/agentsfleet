//! What `exec_command` and `write_stdin` refuse, and what they answer when the
//! executor refuses them.

use serde_json::json;

use super::tests::{CAT, CHARS, FIRST, SESSION_ID, exec, opening, write, writing};
use crate::lease::Lease;
use crate::runtime::ToolErrorCode;
use crate::sandbox::ScriptedExecutor;
use crate::testing::{call, call_in};

#[tokio::test]
async fn should_refuse_a_session_id_that_is_not_a_whole_number() {
    let executor = ScriptedExecutor::new([]);
    let lease = Lease::default();

    for session_id in [json!("1"), json!(-1), json!(1.5)] {
        let arguments = json!({SESSION_ID: session_id, CHARS: "x"});
        let output = call_in(&*write(), &executor, &lease, arguments).await;

        assert_eq!(
            output.error_code,
            Some(ToolErrorCode::InvalidArguments),
            "{session_id}"
        );
    }
    assert!(executor.written().is_empty(), "nothing reached a process");
}

#[tokio::test]
async fn should_refuse_a_session_that_is_not_open() {
    let executor = ScriptedExecutor::new([]);

    let output = call_in(
        &*write(),
        &executor,
        &Lease::default(),
        writing(99, "x", None),
    )
    .await;

    assert_eq!(
        output.text,
        "[session_not_found] session 99 is not an open session"
    );
    let written = executor.written();
    assert!(written.is_empty(), "{written:?}");
}

#[tokio::test]
async fn should_refuse_without_a_sandbox_or_when_the_spawn_is_refused() {
    let lease = Lease::default();

    let opened = call(&*exec(), &lease, opening(CAT, None)).await;
    let written = call(&*write(), &lease, writing(1, "", None)).await;
    let refused = call_in(
        &*exec(),
        &ScriptedExecutor::new([]),
        &lease,
        opening(CAT, None),
    )
    .await;

    assert_eq!(opened.error_code, Some(ToolErrorCode::SandboxUnavailable));
    assert_eq!(written.error_code, Some(ToolErrorCode::SandboxUnavailable));
    assert!(
        refused.text.ends_with(": no scripted process left"),
        "a refused spawn reads the executor's reason, got {:?}",
        refused.text
    );
    assert!(lease.sessions.get(FIRST).is_none());
}
