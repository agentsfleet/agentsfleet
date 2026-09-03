//! The served route table and the generated document describe the same API.
//!
//! # Why this comparison is not redundant
//!
//! `#[utoipa::path(path = "…")]` RESTATES the path string that
//! [`Route::meta`]'s template already carries, and restates the method that
//! [`Route::verbs`] already declares. They are two independent declarations of
//! one fact, and utoipa cannot prove they agree — it never sees the route
//! table. Nothing in the type system binds an annotation to the variant it
//! describes, so the binding is made here, by comparing sets.
//!
//! `route_verbs.rs` is the other half of the same idea and the two are worth
//! reading together: that one grades the DECLARATION against the MOUNT, this
//! one grades the declaration against the DOCUMENT. Between them a route is
//! served, declared and published as one thing, or something fails.
//!
//! # Why path × method and not paths
//!
//! Two route identities can share a template and differ only by method —
//! `PollSession` and `DeleteSession` are one path, as are the connector
//! `Callback`/`Complete` pair and the runner's memory hydrate/capture. A
//! comparison over paths alone would pass while a verb went missing, which is
//! most of what this exists to catch.
#![cfg(all(feature = "test-util", feature = "openapi"))]

use std::collections::BTreeSet;

use afd_api::Route;

/// One operation, as both sides of the comparison spell it.
type Operation = (String, String);

/// What the router serves, from the route table alone.
fn served() -> BTreeSet<Operation> {
    Route::all()
        .flat_map(|route| {
            let template = route.meta().template;
            route
                .verbs()
                .iter()
                .map(move |verb| (template.to_owned(), verb.method().to_string()))
        })
        .collect()
}

/// What the generated document publishes.
///
/// `PathItem` carries one `Option<Operation>` per verb rather than a map, so
/// the five this daemon serves are read by name. HEAD, OPTIONS and TRACE are
/// absent from the list for the same reason they are absent from
/// [`afd_api::route::Verb`]: the daemon has never served them, and a document
/// that published one would be describing a route that answers 405.
fn documented() -> BTreeSet<Operation> {
    afd_api::openapi::document()
        .paths
        .paths
        .into_iter()
        .flat_map(|(path, item)| {
            [
                (&item.get, "GET"),
                (&item.post, "POST"),
                (&item.put, "PUT"),
                (&item.patch, "PATCH"),
                (&item.delete, "DELETE"),
            ]
            .into_iter()
            .filter(|(operation, _)| operation.is_some())
            .map(|(_, method)| (path.clone(), method.to_owned()))
            .collect::<Vec<_>>()
        })
        .collect()
}

/// A readable rendering of one side's excess.
fn render(operations: &BTreeSet<Operation>) -> Vec<String> {
    operations
        .iter()
        .map(|(path, method)| format!("{method} {path}"))
        .collect()
}

/// Every served route is documented, and every documented route is served.
///
/// Both directions in one test because they are one claim: the two sets are
/// equal. Reported separately because the remedies differ — a served route
/// missing from the document needs an annotation and a collector entry, a
/// documented route nobody serves needs the annotation deleted.
#[test]
fn test_coverage_gate_rust_source() {
    let served = served();
    let documented = documented();

    let undocumented: BTreeSet<Operation> = served.difference(&documented).cloned().collect();
    let unserved: BTreeSet<Operation> = documented.difference(&served).cloned().collect();

    assert!(
        undocumented.is_empty() && unserved.is_empty(),
        "the route table and the generated document disagree.\n\
         \n\
         SERVED but NOT DOCUMENTED ({} — the handler needs a `#[utoipa::path]` \
         and an entry in its plane's collector):\n  {}\n\
         \n\
         DOCUMENTED but NOT SERVED ({} — the annotation names a route this \
         daemon does not mount):\n  {}",
        undocumented.len(),
        render(&undocumented).join("\n  "),
        unserved.len(),
        render(&unserved).join("\n  "),
    );
}

/// The route table's own count, so a silent collapse of either side is visible.
///
/// Set equality passes trivially if both sides are empty, and both sides are
/// built by iterators that a refactor could quietly empty. This is the floor
/// that makes the equality above mean something.
#[test]
fn test_the_gate_compares_a_non_empty_inventory() {
    let served = served();
    assert!(
        served.len() > 90,
        "the served inventory collapsed to {} operations; the gate above would \
         pass against an empty document",
        served.len()
    );
}
