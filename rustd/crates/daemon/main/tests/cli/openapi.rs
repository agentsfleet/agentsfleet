//! The `OpenAPI` command reads no environment and reports writer failures.

use af_core::env::MapEnv;
use agentsfleetd::cli::{FAILURE, SUCCESS};

use super::status;

/// `openapi` answers from the route table, not from the environment.
///
/// The dispatch arm is behind `#[cfg(feature = "openapi")]`, so losing it is not
/// a compile error: the parse still succeeds and `run` falls through to
/// `None => check(env)`. An empty environment separates the two. `check` cannot
/// resolve a datastore and exits 1, so SUCCESS is reachable only through the arm
/// that writes the document — which is also the subcommand's own claim, that it
/// needs no datastore and no runtime to answer.
#[cfg(feature = "openapi")]
#[test]
fn test_the_openapi_subcommand_answers_without_an_environment() {
    let status = status(
        &["agentsfleetd", "openapi"],
        &MapEnv::from_pairs(std::iter::empty()),
    );

    assert_eq!(
        status, SUCCESS,
        "the document is a function of the route table, so an empty environment still answers"
    );
}

/// The document reaches its writer whole, as JSON ending in one newline.
#[cfg(feature = "openapi")]
#[test]
fn test_the_openapi_document_reaches_its_writer_whole() {
    let mut written = Vec::new();

    let status = agentsfleetd::cli::openapi_to(&mut written);

    assert_eq!(status, SUCCESS);
    let text = String::from_utf8(written).expect("the document is UTF-8");
    let document: serde_json::Value = serde_json::from_str(&text).expect("the document is JSON");
    assert!(document.get("paths").is_some(), "{text}");
    assert!(
        text.ends_with("}\n"),
        "one closing newline, as `println!` wrote"
    );
}

/// A writer that takes nothing, as stdout redirected to a full disk does.
#[cfg(feature = "openapi")]
struct Full;

#[cfg(feature = "openapi")]
impl std::io::Write for Full {
    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::StorageFull.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A document that cannot be written exits 1 rather than claiming it answered.
#[cfg(feature = "openapi")]
#[test]
fn test_an_openapi_document_that_cannot_be_written_fails() {
    assert_eq!(agentsfleetd::cli::openapi_to(&mut Full), FAILURE);
}
