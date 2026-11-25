//! The drill's one tool call: the file the crash checkpoint must carry to host two.
//!
//! Apart from `model.rs` because the recovery bench reuses that file and makes no
//! tool call.

use super::{Checked, model::Asked};
use serde_json::{Value, json};

impl Asked {
    /// Calls `name` once; the runner runs it, then asks again with its result.
    pub(super) fn call(self, name: &str, arguments: &Value) -> Checked {
        let call = json!({"index":0,"id":format!("call-{name}"),"type":"function",
            "function":{"name":name,"arguments":arguments.to_string()}});
        self.reply(
            json!({"role":"assistant","tool_calls":[call]}),
            "tool_calls",
        )
    }
}
