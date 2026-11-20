//! The web tools: each drafts a request, sends it through the run's
//! [`Outbound`](crate::tools::Outbound) hook, and words what came back.
//!
//! `http_request` and `web_fetch` differ only in the request they draft and
//! how they word the answer; admission, the transport, the mask and the
//! refusal are the hook's, so every tool's request obeys one policy.

pub mod http_request;
pub mod web_fetch;

pub use self::http_request::HttpRequest;
pub use self::web_fetch::WebFetch;

use crate::tool::ToolOutput;
use crate::tools::code;

/// The statuses a tool reports as succeeded.
const SUCCESS: std::ops::Range<u16> = 200..300;

/// `text` as the call's output: succeeded on a 2xx, failed with
/// `upstream_status` otherwise, the model reading the same text either way.
fn answered(status: u16, text: String) -> ToolOutput {
    if SUCCESS.contains(&status) {
        ToolOutput::succeeded(text)
    } else {
        ToolOutput::failed(code::UPSTREAM_STATUS, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::answered;
    use crate::tools::code;

    #[test]
    fn should_answer_2xx_as_succeeded_and_anything_else_as_upstream_status() {
        assert_eq!(answered(204, "Status: 204".to_owned()).error_code, None);
        for status in [199, 301, 404, 500] {
            assert_eq!(
                answered(status, String::new()).error_code,
                Some(code::UPSTREAM_STATUS)
            );
        }
    }
}
