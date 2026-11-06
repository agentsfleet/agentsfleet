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
    use proc_macro2::{TokenStream, TokenTree};
    use std::{fs, path::Path};

    // Value: protects=runtime source cannot silently regain compiled Markdown templates;
    // fails_when=a Markdown file or its include_str loader returns under Rust crates;
    // seam=repository source tree; no_claim=model behavior or native edit authorization.
    #[test]
    fn runtime_sources_have_no_markdown_files_or_markdown_loaders() {
        for source in [
            r#"fn main() { include_str!("resident.md"); }"#,
            r##"fn main() { std::include_str ! (r#"resident.md"#); }"##,
            r#"fn main() { include_str!(concat!("../", "resident.md")); }"#,
            r#"macro_rules! load { () => { include_str!("resident.m\u{64}") }; }"#,
        ] {
            assert!(loads_markdown(source));
        }
        for source in [
            "// include_str! without a macro argument",
            r##"fn main() { let text = r#"include_str!("resident.md")"#; }"##,
            r#"fn main() { include_str!("statement.sql"); }"#,
            r#"fn main() { include_str!(concat!("../schema/", "table.sql")); }"#,
        ] {
            assert!(!loads_markdown(source));
        }
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
                        assert!(
                            !loads_markdown(&source),
                            "{} loads Markdown",
                            path.display()
                        );
                    }
                }
            }
        }
    }

    fn loads_markdown(source: &str) -> bool {
        let mut pending = vec![source.parse::<TokenStream>().expect("valid Rust tokens")];
        while let Some(stream) = pending.pop() {
            let mut tokens = stream.into_iter().peekable();
            while let Some(token) = tokens.next() {
                match token {
                    TokenTree::Ident(name)
                        if name == "include_str"
                            && matches!(tokens.peek(), Some(TokenTree::Punct(p)) if p.as_char() == '!') =>
                    {
                        tokens.next();
                        if let Some(TokenTree::Group(argument)) = tokens.next() {
                            let stream = argument.stream();
                            if markdown_literal(stream.clone()) {
                                return true;
                            }
                            pending.push(stream);
                        }
                    }
                    TokenTree::Group(group) => pending.push(group.stream()),
                    _ => {}
                }
            }
        }
        false
    }

    fn markdown_literal(stream: TokenStream) -> bool {
        stream.into_iter().any(|token| match token {
            TokenTree::Literal(literal) => syn::parse_str::<syn::LitStr>(&literal.to_string())
                .is_ok_and(|text| text.value().contains(".md")),
            TokenTree::Group(group) => markdown_literal(group.stream()),
            _ => false,
        })
    }
}
