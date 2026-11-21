//! A proposal tool: the model proposes a change to a document, the
//! document's own rules decide it, and the last change they accept is kept.
//!
//! ```text
//!  model ─► call(arguments) ─► Document::apply(head, arguments)
//!                                ├─ accepted ─► kept; the next call's head
//!                                └─ refused  ─► "[code] reason" as the call's result;
//!                                               what was kept stays, and the run goes on
//!  embedder, after the run ─► kept() ─► Kept { base: Document::base(), document }
//! ```
//!
//! The harness knows no document. The embedder's [`Document`] names the
//! tool, tells the model its parameters and decides every proposal, and a
//! refused one reaches the model as the call's result so it can correct it in
//! the same run, as `apply_patch` hands back a patch that does
//! not apply. Nothing caps the corrections: the run's own budget does.

use std::fmt;
use std::future::ready;
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::Value;

use crate::tool::{Describe, ErrorCode, Tool, ToolFuture, ToolOutput, ToolSpec};
use crate::tools::code;

/// What the model reads when a proposal is accepted.
const ACCEPTED: &str = "Accepted. It is kept as this run's proposal; a later proposal applies on \
                        top of it and names the same base.";
/// What the model reads when its arguments are not an object to propose.
const NOT_AN_OBJECT: &str = "The arguments are not a JSON object; send the proposal as one.";

/// A document a model proposes changes to, and the rules that decide them.
///
/// Every proposal of a run is made against one revision, [`Document::base`]:
/// the revision the run started from, which an accepted proposal does not
/// move. The document checks a proposal's base in [`Document::apply`], where
/// it reads the proposal, and the tool reports the base beside what it kept.
pub trait Document: Send + Sync + fmt::Debug + 'static {
    /// The whole document an accepted proposal leaves.
    type Accepted: Clone + Send + Sync + fmt::Debug + 'static;
    /// What names the revision a proposal is made against.
    type Revision: Clone + Send + Sync + fmt::Debug + 'static;

    /// The tool as the model is offered it: its name, what it does and its
    /// parameters.
    fn schema(&self) -> ToolSpec<'_>;

    /// The revision every proposal of the run must name.
    fn base(&self) -> Self::Revision;

    /// The document `proposal` leaves when applied to `head`: the document
    /// the last accepted proposal left, or `None` before the first, when the
    /// document starts from its own base.
    ///
    /// # Errors
    /// The rules refuse the proposal: why, for the model to correct it.
    fn apply(
        &self,
        head: Option<&Self::Accepted>,
        proposal: &Value,
    ) -> Result<Self::Accepted, Refusal>;
}

/// Why a document refused a proposal, as the model reads it: `[code] reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    code: ErrorCode,
    reason: String,
}

impl Refusal {
    /// A refusal the model reads as `[code] reason`.
    #[must_use]
    pub fn new(code: ErrorCode, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
        }
    }

    /// The stable code it carries.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// What the model is told to correct.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// The proposal a run kept: the base it was made against and the document it
/// left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept<R, A> {
    /// The revision it was made against.
    pub base: R,
    /// The whole document it left.
    pub document: A,
}

/// The proposal tool over `D`.
///
/// A clone shares what the tool keeps, so the embedder offers one clone to
/// the loop and reads [`Propose::kept`] through another once the run ends.
pub struct Propose<D: Document> {
    document: Arc<D>,
    kept: Arc<Mutex<Option<D::Accepted>>>,
}

impl<D: Document> Propose<D> {
    /// The tool for `document`, with nothing kept yet.
    #[must_use]
    pub fn new(document: D) -> Self {
        Self {
            document: Arc::new(document),
            kept: Arc::new(Mutex::new(None)),
        }
    }

    /// The last proposal the run accepted, with the base it was made against;
    /// `None` when it accepted none.
    #[must_use]
    pub fn kept(&self) -> Option<Kept<D::Revision, D::Accepted>> {
        let kept = self.lock().clone()?;
        Some(Kept {
            base: self.document.base(),
            document: kept,
        })
    }

    /// One call: the proposal applied to what is kept, kept when accepted.
    ///
    /// The lock is held while the document decides, so two calls at once,
    /// from a run's children, apply one after the other and neither is lost.
    fn propose(&self, arguments: &Value) -> ToolOutput {
        if !arguments.is_object() {
            return ToolOutput::failed(code::INVALID_ARGUMENTS, NOT_AN_OBJECT);
        }
        let mut kept = self.lock();
        match self.document.apply(kept.as_ref(), arguments) {
            Ok(accepted) => {
                *kept = Some(accepted);
                ToolOutput::succeeded(ACCEPTED)
            }
            Err(refusal) => ToolOutput::failed(refusal.code, &refusal.reason),
        }
    }

    /// What is kept. A call that panicked while holding it left it as it
    /// was, since a proposal is kept only after the document accepted it.
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<D::Accepted>> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<D: Document> Clone for Propose<D> {
    fn clone(&self) -> Self {
        Self {
            document: Arc::clone(&self.document),
            kept: Arc::clone(&self.kept),
        }
    }
}

impl<D: Document> fmt::Debug for Propose<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Propose")
            .field("document", &self.document)
            .field("kept", &self.lock().is_some())
            .finish()
    }
}

impl<D: Document> Describe for Propose<D> {
    fn spec(&self) -> ToolSpec<'_> {
        self.document.schema()
    }
}

/// The tool needs nothing a call is lent, so any loop can hold it.
impl<D: Document, Cx> Tool<Cx> for Propose<D> {
    fn call<'a>(&'a self, arguments: &'a Value, _cx: Cx) -> ToolFuture<'a>
    where
        Cx: 'a,
    {
        Box::pin(ready(self.propose(arguments)))
    }
}

#[cfg(test)]
mod tests;
