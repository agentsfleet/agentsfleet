//! What an operation publishes for a refusal is what the route table decides.
//!
//! # Why this is not a comparison against the error registry
//!
//! The obvious reading of "documented codes match the handler's refusals" is a
//! per-handler enumeration of every `UZ-*` it can raise, and that is not
//! statically derivable: a handler's refusals come from whichever plane error
//! its service layer returns, through `Refusable::status()`, several calls
//! down. Deriving that would mean an analysis the compiler does not offer.
//!
//! What IS decided in one place, and decided BEFORE any handler runs, is the
//! authenticator's half. `RouteMeta` says whether a route is guarded and which
//! capability it needs; the layers in front of the handler answer 401 and 403
//! from exactly those two facts. So three codes are not a matter of taste:
//!
//!   * a route whose guard is not `Open` can always refuse an absent or
//!     unacceptable credential — 401, `AUTH_UNAUTHORIZED`
//!   * a route with a non-empty scope rung can always refuse a credential that
//!     lacks it — 403, `AUTH_FORBIDDEN`, from `require_scope`
//!   * anything that reaches a plane can fail — 500
//!
//! The probes are the exception to that last one, and not a grudging one: they
//! answer from process state without touching a service, so `/healthz` can only
//! say 200 and `/readyz` only 200 or 503. Documenting a 500 they cannot produce
//! would be the same defect in the other direction. `RouteClass::Ops` is the
//! table's own name for that class, so it is what this reads.
//!
//! An operation that omits one of those documents a refusal a caller will meet
//! and was not told about, which is the defect the committed contract had at
//! scale: most of its operations listed a success and `default` and nothing in
//! between.
#![cfg(all(feature = "test-util", feature = "openapi"))]

use std::collections::{BTreeMap, BTreeSet};

use afd_api::Route;
use afd_api::route::{Guard, RouteClass};

/// Refused for want of a credential.
const UNAUTHORIZED: u16 = 401;

/// Refused for want of a capability.
const FORBIDDEN: u16 = 403;

/// The daemon could not answer.
const INTERNAL: u16 = 500;

/// One operation, keyed as both sides spell it.
type Operation = (String, String);

/// The codes each operation publishes, read out of the generated document.
fn published() -> BTreeMap<Operation, BTreeSet<u16>> {
    let mut out = BTreeMap::new();
    for (path, item) in afd_api::openapi::document().paths.paths {
        for (operation, method) in [
            (&item.get, "GET"),
            (&item.post, "POST"),
            (&item.put, "PUT"),
            (&item.patch, "PATCH"),
            (&item.delete, "DELETE"),
        ] {
            if let Some(operation) = operation.as_ref() {
                let codes = operation
                    .responses
                    .responses
                    .keys()
                    .filter_map(|code| code.parse::<u16>().ok())
                    .collect();
                out.insert((path.clone(), method.to_owned()), codes);
            }
        }
    }
    out
}

/// Every operation publishes the refusals its own route metadata implies.
#[test]
fn test_documented_codes_match_refusals() {
    let published = published();
    let mut missing = Vec::new();

    for route in Route::all() {
        let meta = route.meta();
        for verb in route.verbs() {
            let method = verb.method();
            let mut required = Vec::new();
            if meta.class != RouteClass::Ops {
                required.push(INTERNAL);
            }
            if meta.guard != Guard::Open {
                required.push(UNAUTHORIZED);
            }
            if !meta.scopes.required(&method).is_empty() {
                required.push(FORBIDDEN);
            }
            let key = (meta.template.to_owned(), method.to_string());
            let Some(codes) = published.get(&key) else {
                continue; // the coverage gate owns this direction
            };
            for code in required {
                if !codes.contains(&code) {
                    missing.push(format!(
                        "{method} {} does not publish {code}",
                        meta.template
                    ));
                }
            }
        }
    }

    assert!(
        missing.is_empty(),
        "an operation omits a refusal its route metadata guarantees a caller \
         can meet:\n  {}",
        missing.join("\n  ")
    );
}
