//! What one call hands back to the loop: the text the model reads, why it
//! failed, and the image it read.

use std::fmt;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::agents::Order;

/// What a call to a tool the loop does not hold reads, after its name.
const NOT_OFFERED: &str = "is not one of this run's tools";
/// What a call from a step cut at the output limit reads, after its name.
const CUT: &str = "was not run: the step reached the model's output limit and its \
                   arguments may be incomplete; call it again with shorter arguments";

/// Why a call failed, in the spelling the model reads.
///
/// An embedder keeps its own closed set of codes and converts each into this
/// one, so the harness can refuse a call in the same words without knowing
/// that set. Two codes are equal when they are spelled alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ErrorCode(&'static str);

impl ErrorCode {
    /// The model called a tool the loop does not hold.
    pub const NOT_OFFERED: Self = Self("tool_not_offered");
    /// The model's step stopped at its output limit, so the call may have
    /// been cut mid-argument and was not run.
    pub const OUTPUT_LIMIT_REACHED: Self = Self("output_limit_reached");
    /// What the call waited on ended before it answered.
    pub const INTERRUPTED: Self = Self("interrupted");
    /// The run already has as many children open, or started, as its caps
    /// allow, or its children are paused.
    pub const CHILD_CAP_REACHED: Self = Self("child_cap_reached");
    /// A child asked for a tool its parent does not hold.
    pub const CHILD_TOOL_NOT_HELD: Self = Self("child_tool_not_held");
    /// No child of the run has the id named.
    pub const CHILD_NOT_FOUND: Self = Self("child_not_found");
    /// Another call of the same step already asked the person a question.
    pub const QUESTION_PENDING: Self = Self("question_pending");

    /// The code spelled `spelling`.
    #[must_use]
    pub const fn new(spelling: &'static str) -> Self {
        Self(spelling)
    }

    /// The spelling the output text carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// The kinds of image a call can hand the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageKind {
    /// Portable Network Graphics.
    Png,
    /// Joint Photographic Experts Group.
    Jpeg,
    /// Graphics Interchange Format.
    Gif,
    /// WebP.
    Webp,
}

impl ImageKind {
    /// The media type the kind is named by.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }
}

/// An image a call read, for the model to see with that call's result.
///
/// It serializes with the conversation that carries it, so a resumed loop
/// hands the model the image it saw before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    /// What kind of image it is.
    pub kind: ImageKind,
    /// The bytes, as the file holds them.
    pub data: Bytes,
}

/// What one call hands back to the loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    /// What the model reads back.
    pub text: String,
    /// The process's exit code, for a call that ran one.
    pub exit_code: Option<i32>,
    /// Why the call failed, for one that did.
    pub error_code: Option<ErrorCode>,
    /// The image the call read, which rides its result alone.
    pub image: Option<Image>,
    /// The question a call asks the person: the loop ends
    /// [`crate::Ending::Waiting`] once the step's other calls ran, and the
    /// call's result is the answer the run is resumed with.
    pub asks: Option<String>,
    /// What a subagent call asks of the loop, which runs it in the calling
    /// loop's place before the call ends.
    pub(crate) order: Option<Box<Order>>,
}

impl ToolOutput {
    /// A call that succeeded with `text`.
    #[must_use]
    pub fn succeeded(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            exit_code: None,
            error_code: None,
            image: None,
            asks: None,
            order: None,
        }
    }

    /// A call that failed: the model reads `[code] detail`, so the code
    /// reaches it with the reason rather than as a bare string.
    #[must_use]
    pub fn failed(code: impl Into<ErrorCode>, detail: &str) -> Self {
        let code = code.into();
        Self {
            text: format!("[{code}] {detail}"),
            exit_code: None,
            error_code: Some(code),
            image: None,
            asks: None,
            order: None,
        }
    }

    /// A call that asks the person `question`: the model reads the answer,
    /// once the embedder resumes the run with it.
    #[must_use]
    pub fn asks(question: impl Into<String>) -> Self {
        let question = question.into();
        Self {
            text: question.clone(),
            exit_code: None,
            error_code: None,
            image: None,
            asks: Some(question),
            order: None,
        }
    }

    /// A subagent call's answer: `order`, which the loop runs in the calling
    /// loop's place before the call ends, and whose answer the model reads.
    #[must_use]
    pub fn ordering(order: Order) -> Self {
        Self {
            order: Some(Box::new(order)),
            ..Self::succeeded(String::new())
        }
    }

    /// A call to `name`, which the loop holds no tool for. The model reads
    /// it as a tool error and the next step still runs.
    #[must_use]
    pub fn not_offered(name: &str) -> Self {
        Self::failed(ErrorCode::NOT_OFFERED, &format!("{name} {NOT_OFFERED}"))
    }

    /// A call to `name` from a step the provider cut at its output limit:
    /// its arguments may be incomplete, so it is never run.
    #[must_use]
    pub fn cut(name: &str) -> Self {
        Self::failed(ErrorCode::OUTPUT_LIMIT_REACHED, &format!("{name} {CUT}"))
    }

    /// The same output, with the image the call read.
    #[must_use]
    pub fn with_image(mut self, image: Image) -> Self {
        self.image = Some(image);
        self
    }
}
