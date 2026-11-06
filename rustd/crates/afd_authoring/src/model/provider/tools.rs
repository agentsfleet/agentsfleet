//! Native setup tool schema; the person supplies conversational instructions.

use super::{KEY_DESCRIPTION, KEY_TYPE};
use serde_json::{Value, json};

pub(in crate::model) const TOOL: &str = "propose_setup";

pub(in crate::model) fn tool(editing: bool) -> Value {
    json!({
        KEY_TYPE:"function", "name":TOOL,
        KEY_DESCRIPTION:"Return a setup proposal for native validation. This tool does not save or run a fleet.",
        "parameters":super::schema::parameters(editing)
    })
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "source-tree fixture failures must fail the regression guard"
)]
mod tests {
    use std::{fs, path::Path};

    // Value: protects=runtime source cannot silently regain compiled Markdown templates;
    // fails_when=a Markdown file or its include_str loader returns under Rust crates;
    // seam=repository source tree; no_claim=model behavior or native edit authorization.
    #[test]
    fn runtime_sources_have_no_markdown_files_or_markdown_loaders() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Rust crates directory");
        let mut pending = vec![crates.to_path_buf()];
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory).expect("Rust directory") {
                let entry = entry.expect("Rust source entry");
                let kind = entry.file_type().expect("source file type");
                let path = entry.path();
                if kind.is_dir() {
                    pending.push(path);
                } else if kind.is_file() {
                    assert_ne!(
                        path.extension().and_then(|v| v.to_str()),
                        Some("md"),
                        "{}",
                        path.display()
                    );
                    if path.extension().and_then(|v| v.to_str()) == Some("rs") {
                        let source = fs::read_to_string(&path).expect("Rust source");
                        for invocation in source.split("include_str!").skip(1) {
                            let argument = invocation.split_once(')').expect("macro argument").0;
                            assert!(
                                !argument.contains(".md"),
                                "{} loads Markdown",
                                path.display()
                            );
                        }
                    }
                }
            }
        }
    }
}
