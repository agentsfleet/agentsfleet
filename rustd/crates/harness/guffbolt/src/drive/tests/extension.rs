//! What an extension may add at each point of the loop, and the order it hears
//! them in.

use std::time::Duration;

use crate::extension::{Answer, Budget, Extension};
use crate::turn::Ending;

use super::drive;
use super::fixture::{Failure, Message, Said, Scripted, Step, answer, answered, calling};

/// An extension that records when each hook ran, and ends or continues on cue.
#[derive(Default)]
struct Recording {
    seen: Vec<String>,
    end_before: Option<u64>,
    end_after_turn: bool,
    continue_once: Option<&'static str>,
    end_answer: Option<&'static str>,
    end_after_group: bool,
    budget: Option<Budget>,
    spent: bool,
}

impl Extension<Scripted> for Recording {
    fn budget(&self, _conversation: &Scripted) -> Budget {
        self.budget.unwrap_or(Budget::Unbounded)
    }

    fn budget_spent(&mut self, _conversation: &mut Scripted) {
        self.spent = true;
    }

    fn before_model(
        &mut self,
        _conversation: &mut Scripted,
        number: u64,
    ) -> impl Future<Output = Option<Ending<Failure>>> + Send {
        self.seen.push(format!("before {number}"));
        std::future::ready(
            (self.end_before == Some(number))
                .then_some(Ending::Answered("from the extension".to_owned())),
        )
    }

    fn after_turn(&mut self, _conversation: &Scripted, turn: &Said) -> Option<Ending<Failure>> {
        self.seen.push(format!("turn {}", turn.calls.len()));
        self.end_after_turn.then_some(Ending::Stopped)
    }

    fn on_answer(
        &mut self,
        _conversation: &mut Scripted,
        answer: &str,
    ) -> impl Future<Output = Answer<Failure>> + Send {
        self.seen.push(format!("answer {answer}"));
        std::future::ready(match (self.end_answer, self.continue_once.take()) {
            (Some(text), _) => Answer::End(Ending::Answered(text.to_owned())),
            (None, Some(why)) => Answer::Continue(why.to_owned()),
            (None, None) => Answer::Accept,
        })
    }

    fn after_group(
        &mut self,
        _conversation: &mut Scripted,
    ) -> impl Future<Output = Option<Ending<Failure>>> + Send {
        self.seen.push("group".to_owned());
        std::future::ready(self.end_after_group.then_some(Ending::Stopped))
    }
}

#[tokio::test]
async fn an_extension_hears_every_point_in_order() {
    let mut run = Scripted::new([calling(1), answer("Done.")]);
    let mut extension = Recording::default();
    drive(&mut run, &mut extension).await;
    assert_eq!(
        extension.seen,
        [
            "before 1",
            "turn 1",
            "group",
            "before 2",
            "turn 0",
            "answer Done."
        ]
    );
}

#[tokio::test]
async fn an_extension_that_ends_before_the_model_makes_no_call() {
    let mut run = Scripted::new([answer("unreachable")]);
    let mut extension = Recording {
        end_before: Some(1),
        ..Recording::default()
    };
    let ending = drive(&mut run, &mut extension).await;
    assert_eq!(answered(&ending), Some("from the extension"));
    assert_eq!(run.sent.len(), 0, "no call is made");
}

#[tokio::test]
async fn an_extension_can_refuse_a_turn_before_its_calls_run() {
    let mut run = Scripted::new([calling(2)]);
    let mut extension = Recording {
        end_after_turn: true,
        ..Recording::default()
    };
    assert!(matches!(
        drive(&mut run, &mut extension).await,
        Ending::Stopped
    ));
    assert_eq!(run.results(), 0, "no call of a refused turn runs");
}

#[tokio::test]
async fn a_continued_answer_stays_in_the_conversation_and_the_model_is_told_why() {
    let mut run = Scripted::new([answer("Halfway."), answer("Finished.")]);
    let mut extension = Recording {
        continue_once: Some("The goal remains incomplete."),
        ..Recording::default()
    };
    let ending = drive(&mut run, &mut extension).await;
    assert_eq!(answered(&ending), Some("Finished."));
    assert_eq!(
        run.sent.get(1),
        Some(&vec![
            Message::User("deploy the fix".to_owned()),
            Message::Assistant("Halfway.".to_owned()),
            Message::User("The goal remains incomplete.".to_owned()),
        ])
    );
}

#[tokio::test]
async fn an_extension_can_end_the_loop_in_place_of_an_answer() {
    let mut run = Scripted::new([answer("Done.")]);
    let mut extension = Recording {
        end_answer: Some("The extension's own ending."),
        ..Recording::default()
    };
    let ending = drive(&mut run, &mut extension).await;
    assert_eq!(answered(&ending), Some("The extension's own ending."));
    assert!(
        !run.messages
            .contains(&Message::Assistant("Done.".to_owned())),
        "the loop remembers nothing of an answer the extension replaced"
    );
}

#[tokio::test]
async fn an_extension_can_end_the_loop_after_a_group() {
    let mut run = Scripted::new([calling(1), answer("unreachable")]);
    let mut extension = Recording {
        end_after_group: true,
        ..Recording::default()
    };
    assert!(matches!(
        drive(&mut run, &mut extension).await,
        Ending::Stopped
    ));
    assert_eq!(run.sent.len(), 1);
}

#[tokio::test]
async fn a_spent_budget_stops_the_loop_before_any_call() {
    let mut run = Scripted::new([answer("unreachable")]);
    let mut extension = Recording {
        budget: Some(Budget::Spent),
        ..Recording::default()
    };
    assert!(matches!(
        drive(&mut run, &mut extension).await,
        Ending::Stopped
    ));
    assert_eq!(run.sent.len(), 0, "no call is made");
}

#[tokio::test(start_paused = true)]
async fn a_budget_that_runs_out_mid_call_stops_the_loop_and_says_so() {
    let mut run = Scripted::new([Step::Hang]);
    let mut extension = Recording {
        budget: Some(Budget::Within(Duration::from_secs(30))),
        ..Recording::default()
    };
    assert!(matches!(
        drive(&mut run, &mut extension).await,
        Ending::Stopped
    ));
    assert!(extension.spent, "the extension hears its budget ran out");
}

#[tokio::test(start_paused = true)]
async fn a_loop_that_ends_within_its_budget_ends_as_it_would_unbounded() {
    let mut run = Scripted::new([answer("Done.")]);
    let mut extension = Recording {
        budget: Some(Budget::Within(Duration::from_secs(30))),
        ..Recording::default()
    };
    let ending = drive(&mut run, &mut extension).await;
    assert_eq!(answered(&ending), Some("Done."));
    assert!(!extension.spent, "the budget did not run out");
}
