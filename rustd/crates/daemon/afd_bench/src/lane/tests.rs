//! A lost task is named, never swallowed.

#![expect(
    clippy::panic,
    reason = "the lost task is lost by panicking on purpose"
)]

#[tokio::test]
async fn a_panicked_task_is_reported_as_lost_under_its_role() {
    // The one way a lane's task is lost in practice: it panicked.
    let panicked = tokio::spawn(async { std::panic::panic_any::<&str>("a lane task panicked") });

    let lost = super::joined::<()>(panicked.await, "tail viewer");

    assert!(
        matches!(
            lost.as_ref().err().map(crate::Error::kind),
            Some(crate::error::ErrorKind::TaskLost {
                role: "tail viewer"
            })
        ),
        "{lost:?}"
    );
}

#[tokio::test]
async fn a_finished_task_hands_back_its_value() {
    let finished = tokio::spawn(async { 7_u8 });

    assert!(matches!(super::joined(finished.await, "runner"), Ok(7)));
}
