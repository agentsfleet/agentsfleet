#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{Access, Reach, RepositoryBinding, RepositoryPublication};
use crate::error::ErrorKind;

/// A refused scope's error, so each refusal's kind is told apart.
fn refused(scope: &str) -> crate::Error {
    RepositoryBinding::parse(scope).expect_err("the scope is refused")
}

#[test]
fn a_read_scope_takes_no_base() {
    let binding = RepositoryBinding::parse(
        r#"{"repositories": ["agentsfleet/agentsfleet"], "access": "read"}"#,
    )
    .expect("a read scope is complete without a base");

    assert_eq!(binding.reach(), &Reach::Read);
    assert_eq!(binding.access(), Access::Read);
    assert_eq!(binding.publication(), RepositoryPublication::EventDraft);
}

#[test]
fn a_write_scope_carries_its_base_and_reads_back_from_its_record() {
    let binding =
        RepositoryBinding::parse(r#"{"repositories": ["a/b"], "access": "write", "base": "dev"}"#)
            .expect("a write scope with a base is complete");

    assert_eq!(binding.reach(), &Reach::Write { base: "dev".into() });
    let recorded = serde_json::to_string(&binding.recorded()).expect("a record serializes");
    assert_eq!(
        recorded,
        r#"{"repositories":["a/b"],"access":"write","base":"dev"}"#
    );
    let again = RepositoryBinding::parse(&recorded).expect("a stored scope reads back");
    assert_eq!(again, binding);
}

#[test]
fn a_list_without_an_access_level_is_refused() {
    assert!(matches!(
        refused(r#"{"repositories": ["a/b"]}"#).kind(),
        ErrorKind::InvalidRepositoryBinding { .. }
    ));
}

#[test]
fn an_access_level_without_a_list_is_refused() {
    assert!(matches!(
        refused(r#"{"access": "read"}"#).kind(),
        ErrorKind::InvalidRepositoryBinding { .. }
    ));
}

#[test]
fn an_empty_scope_is_refused() {
    assert!(matches!(
        refused("{}").kind(),
        ErrorKind::InvalidRepositoryBinding { .. }
    ));
}

#[test]
fn an_empty_list_is_refused_because_a_token_scoped_to_nothing_cannot_mint() {
    assert!(matches!(
        refused(r#"{"repositories": [], "access": "read"}"#).kind(),
        ErrorKind::OutOfBounds { .. }
    ));
}

#[test]
fn a_read_scope_that_names_a_base_is_refused() {
    assert!(matches!(
        refused(r#"{"repositories": ["a/b"], "access": "read", "base": "main"}"#).kind(),
        ErrorKind::InvalidRepositoryBinding { .. }
    ));
}

#[test]
fn a_write_scope_without_a_base_is_refused() {
    assert!(matches!(
        refused(r#"{"repositories": ["a/b"], "access": "write"}"#).kind(),
        ErrorKind::InvalidRepositoryBinding { .. }
    ));
}

#[test]
fn an_unknown_key_is_refused_by_its_path() {
    let failure =
        refused(r#"{"repositories": ["a/b"], "access": "read", "repository_base": "main"}"#);
    assert!(
        matches!(failure.kind(), ErrorKind::UnsupportedKey { path } if &**path == "repository_base"),
        "{failure:?}"
    );
}

#[test]
fn an_unknown_access_level_names_the_ones_that_exist() {
    let failure = RepositoryBinding::parse(r#"{"repositories": ["a/b"], "access": "admin"}"#)
        .expect_err("`admin` is not an access level");

    assert!(matches!(failure.kind(), ErrorKind::InvalidFieldType { .. }));
}

#[test]
fn a_repository_that_is_not_owner_and_name_is_refused() {
    for repository in ["a", "a/", "/b", "a/b/c", "a /b", "ünï/cødé x"] {
        let scope = format!(r#"{{"repositories": ["{repository}"], "access": "read"}}"#);
        assert!(
            matches!(refused(&scope).kind(), ErrorKind::OutOfBounds { .. }),
            "{repository}"
        );
    }
}

#[test]
fn existing_pr_publication_needs_write_and_its_workflows() {
    let workflow = r#"{"path": ".github/workflows/ci.yml", "sha256": "0000000000000000000000000000000000000000000000000000000000000000"}"#;
    let read = format!(
        r#"{{"repositories": ["a/b"], "access": "read", "publication": "existing_pr", "validation_workflows": [{workflow}]}}"#
    );
    assert!(matches!(
        refused(&read).kind(),
        ErrorKind::InvalidRepositoryBinding { .. }
    ));

    let bare = r#"{"repositories": ["a/b"], "access": "write", "base": "main", "publication": "existing_pr"}"#;
    assert!(matches!(
        refused(bare).kind(),
        ErrorKind::InvalidRepositoryBinding { .. }
    ));

    let twice = format!(
        r#"{{"repositories": ["a/b"], "access": "write", "base": "main", "publication": "existing_pr", "validation_workflows": [{workflow}, {workflow}]}}"#
    );
    assert!(matches!(
        refused(&twice).kind(),
        ErrorKind::InvalidRepositoryBinding { .. }
    ));

    let once = format!(
        r#"{{"repositories": ["a/b"], "access": "write", "base": "main", "publication": "existing_pr", "validation_workflows": [{workflow}]}}"#
    );
    let binding = RepositoryBinding::parse(&once).expect("a complete existing-PR scope");
    assert_eq!(binding.publication(), RepositoryPublication::ExistingPr);
    assert_eq!(binding.validation_workflows().len(), 1);
}

#[test]
fn a_branch_name_may_not_reach_outside_itself() {
    let binding = |base: &str| {
        RepositoryBinding::parse(&format!(
            r#"{{"repositories": ["a/b"], "access": "write", "base": "{base}"}}"#
        ))
    };

    binding("main").expect("a plain branch");
    binding("release/2.0").expect("a branch under a folder");
    for refused_base in [
        "../etc",
        "main@{1}",
        "main.lock",
        "main~1",
        "/main",
        "main/",
        "",
    ] {
        assert!(binding(refused_base).is_err(), "{refused_base:?}");
    }
}

#[test]
fn a_narrowed_binding_reads_one_repository() {
    let binding = RepositoryBinding::parse(
        r#"{"repositories": ["Org/App", "org/other"], "access": "write", "base": "main"}"#,
    )
    .expect("a write scope");

    let narrowed = binding
        .read_repository("org/app")
        .expect("held, compared without case");
    assert_eq!(narrowed.repositories(), [Box::from("Org/App")]);
    assert_eq!(narrowed.reach(), &Reach::Read);
    assert!(binding.read_repository("org/missing").is_none());
    assert_eq!(binding.read_only().reach(), &Reach::Read);
}
