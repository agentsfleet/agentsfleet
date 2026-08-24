//! The attribute keys this daemon emits, spelled once.
//!
//! Mirrors `observability/semconv.zig`, which exists for the reason this does:
//! a key that differs by one character between two emitters is two series in
//! the backend, and nothing reports that as an error. The Zig file is the
//! registry of record while the port runs, and
//! `test_semconv_matches_zig` reads it.
//!
//! # Why the HTTP keys are the only ones here yet
//!
//! Codes and attributes are added as the milestone that emits them lands. The
//! `gen_ai.*` and `agentsfleet.billing.*` families belong to work this port has
//! not reached, and an unreferenced constant is dead code that looks like
//! coverage.

/// The instrumentation scope, and the default `service.name`.
pub const SCOPE_NAME: &str = "agentsfleetd";

/// The namespace every signal from this product carries.
pub const SERVICE_NAMESPACE: &str = "agentsfleet";

/// Resource key for the namespace above.
pub const RESOURCE_SERVICE_NAMESPACE: &str = "service.namespace";

/// The request method, upper-case, as OpenTelemetry spells it.
pub const ATTR_HTTP_REQUEST_METHOD: &str = "http.request.method";

/// The matched route TEMPLATE — never a concrete path.
///
/// This is the low-cardinality half of the contract. A real path carries
/// workspace, fleet and lease identifiers, so exporting one would put tenant
/// identity into span attributes AND give the backend a distinct route value
/// per request, which is the same thing as having no route dimension at all.
pub const ATTR_HTTP_ROUTE: &str = "http.route";

/// The response status code.
pub const ATTR_HTTP_RESPONSE_STATUS_CODE: &str = "http.response.status_code";
