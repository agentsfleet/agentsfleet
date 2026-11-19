//! `ask_user`: a question for the person, which the run waits on.
//!
//! The call answers [`ToolOutput::asks`]; the loop runs the turn's other
//! calls, then ends [`crate::Ending::Waiting`] with the call's id and the
//! question. How the question reaches the person is the embedder's: a chat
//! message, a notification, an approval card. Nothing waits inside the run,
//! so a run that holds a lease or a machine gives it back while the person
//! thinks. The answer arrives later through [`crate::DriveState::resume`],
//! as this call's result, and the run goes on from there.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::tool::ToolOutput;
use crate::tools::{Context, Handler};

/// The name the model calls it by.
pub const ASK_USER: &str = "ask_user";

/// `ask_user`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Question {
    /// The question, as the person reads it: one question, with what they
    /// need to answer it.
    question: String,
    /// Answers to offer, when the question has a few; the person may still
    /// answer in their own words.
    #[serde(default)]
    options: Vec<String>,
}

/// Asks the person a question and waits for the answer.
#[derive(Debug, Clone, Copy, Default)]
pub struct AskUser;

impl Handler for AskUser {
    const NAME: &'static str = ASK_USER;
    const DESCRIPTION: &'static str = "Ask the person a question you cannot answer from what \
        you can read or run, and wait for their answer, which is this call's result. The run \
        pauses until they answer. Ask one question at a time.";
    type Arguments = Question;

    fn run(
        &self,
        arguments: Question,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        let Question { question, options } = arguments;
        let asked = if options.is_empty() {
            question
        } else {
            let offered: Vec<String> = options
                .iter()
                .enumerate()
                .map(|(number, option)| format!("{}. {option}", number + 1))
                .collect();
            format!("{question}\n\n{}", offered.join("\n"))
        };
        std::future::ready(ToolOutput::asks(asked))
    }
}
