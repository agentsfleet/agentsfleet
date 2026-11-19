//! A replacement preserves prior answers without skipping first-call hooks.

use crate::{DriveState, Ending, Extension, Plain, drive};

use super::fixture::{Scripted, answered};
use super::script::{Failure, answer};

#[derive(Default)]
struct Boundary {
    calls: Vec<(u64, u64)>,
}

impl Extension<Scripted> for Boundary {
    fn before_model(
        &mut self,
        _conversation: &mut Scripted,
        number: u64,
        state: &DriveState,
    ) -> impl Future<Output = Option<Ending<Failure>>> {
        self.calls.push((number, state.completed_turns()));
        std::future::ready((number == 2).then_some(Ending::Stopped))
    }
}

#[tokio::test]
async fn recovered_driver_preserves_accepted_answers_and_first_attempt_reconciliation() {
    let mut first = Scripted::new([answer("Answer A"), answer("unreachable")]);
    first.during.insert(1, vec!["Follow up"]);
    let mut state = DriveState::default();
    let mut boundary = Boundary::default();
    assert!(matches!(
        drive(&mut first, &mut boundary, &mut state).await,
        Ending::Stopped
    ));
    assert_eq!(boundary.calls, [(1, 0), (2, 1)]);
    assert_eq!(state.answers(), ["Answer A"]);

    let mut replacement = Scripted::new([answer("Answer B")]);
    replacement.transcript = first.transcript;
    let ending = drive(&mut replacement, &mut Plain, &mut state).await;
    assert_eq!(answered(&ending), Some("Answer A\n\nAnswer B"));
    assert_eq!(state.completed_turns(), 2);
    assert_eq!(state.answers(), ["Answer A", "Answer B"]);
    // A new extension receives number one with the saved completed count.
    let mut next = Scripted::new([answer("Answer C")]);
    let mut hook = Boundary::default();
    drive(&mut next, &mut hook, &mut state).await;
    assert_eq!(hook.calls, [(1, 2)]);
}
