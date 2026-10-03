use std::borrow::Cow;

use super::Prompt;
use crate::fixture::{lease, unbounded};

#[test]
fn should_ask_the_events_message_under_the_installed_instructions() {
    let prompt = Prompt::new(&lease(&[], unbounded()));

    assert_eq!(prompt.message, "triage the failed run");
    assert_eq!(
        prompt.instructions,
        "## Installed instructions\n\nRead the run."
    );
}

#[test]
fn should_ask_the_whole_request_when_it_carries_no_message_string() {
    for request in [
        "not json",
        "{\"message\": 7}",
        "{\"other\": \"x\"}",
        "[\"message\"]",
    ] {
        let mut lease = lease(&[], unbounded());
        lease.event.request_json = Cow::Borrowed(request);

        assert_eq!(Prompt::new(&lease).message, request);
    }
}

#[test]
fn should_send_no_system_prompt_without_instructions() {
    let mut lease = lease(&[], unbounded());
    lease.instructions = Cow::Borrowed("");

    assert_eq!(Prompt::new(&lease).instructions, "");
}
