use super::*;
use crate::client::Verb;

const PANIC_MESSAGE: &str = "handler fixture broke";

fn call(verb: Verb) -> Call {
    Call {
        verb,
        path: "/fake".into(),
        body: None,
    }
}

fn failure_message(panic: Box<dyn std::any::Any + Send>) -> String {
    *panic.downcast::<String>().unwrap()
}

#[tokio::test]
async fn should_answer_500_and_fail_teardown_when_a_handler_panics() {
    let (api, mut calls) = fake(|_| panic!("{PANIC_MESSAGE}"));
    let serving = tokio::spawn(async move { api.send(call(Verb::Lease)).await });
    let failure = bounded("panicking handler response", serving)
        .await
        .unwrap()
        .unwrap_err();
    assert!(
        failure.to_string().contains("lease call (500)"),
        "{failure}"
    );
    assert_eq!(calls.recv().await.unwrap().verb, Verb::Lease);
    let panic = catch_unwind(AssertUnwindSafe(|| drop(calls))).unwrap_err();
    let message = failure_message(panic);
    assert!(message.contains(PANIC_MESSAGE), "{message}");
    assert!(message.contains("lease /fake"), "{message}");
}

#[tokio::test]
async fn should_record_non_string_panics_without_losing_the_response() {
    let (api, calls) = fake(|_| std::panic::panic_any(7_u8));
    let failure = api.send(call(Verb::Me)).await.unwrap_err();
    assert!(failure.to_string().contains("me call (500)"), "{failure}");
    let panic = catch_unwind(AssertUnwindSafe(|| drop(calls))).unwrap_err();
    assert!(failure_message(panic).contains("non-string panic"));
}

#[tokio::test]
async fn should_reject_an_unexpected_request_and_fail_the_owning_test() {
    let (api, calls) = fake(crate::test_support::daemon(|_| None));
    let failure = api.send(call(Verb::AuthoringModel)).await.unwrap_err();
    assert!(
        failure.to_string().contains("authoring_model call (500)"),
        "{failure}"
    );
    let panic = catch_unwind(AssertUnwindSafe(|| drop(calls))).unwrap_err();
    assert!(failure_message(panic).contains("unexpected fake daemon request"));
}

#[tokio::test]
async fn should_keep_intentional_failures_out_of_the_internal_failure_record() {
    let (api, mut calls) = fake(|call| Answer::Fail(error::refused(call.verb, 401, None)));
    let failure = api.send(call(Verb::Me)).await.unwrap_err();
    assert!(failure.is_unauthorized());
    assert_eq!(drain(&mut calls).len(), 1);
    drop(calls);
}

#[tokio::test(start_paused = true)]
#[should_panic(expected = "waiting for lease report")]
async fn should_name_the_stuck_operation_when_the_bound_expires() {
    bounded("lease report", std::future::pending::<()>()).await;
}

#[tokio::test(start_paused = true)]
async fn should_return_the_completed_operation_without_advancing_time() {
    let start = tokio::time::Instant::now();
    assert_eq!(bounded("ready answer", async { 42 }).await, 42);
    assert_eq!(tokio::time::Instant::now(), start);
}

#[tokio::test]
async fn should_preserve_the_original_test_panic_during_teardown() {
    let (api, calls) = fake(|_| panic!("{PANIC_MESSAGE}"));
    drop(api.send(call(Verb::Me)).await);
    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _calls = calls;
        panic!("original assertion");
    }))
    .unwrap_err();
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"original assertion"));
}
