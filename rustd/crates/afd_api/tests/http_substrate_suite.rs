//! HTTP substrate and composition-root regression suite.

mod harness;

#[path = "admission_ceiling.rs"]
mod admission_ceiling;
#[path = "header_limit.rs"]
mod header_limit;
#[path = "http_plane_dependency_graph.rs"]
mod http_plane_dependency_graph;
#[path = "person_policy.rs"]
mod person_policy;
#[path = "problem_json_envelope.rs"]
mod problem_json_envelope;
#[path = "protocol_negotiation.rs"]
mod protocol_negotiation;
#[path = "request_id.rs"]
mod request_id;
// Gated on the feature that generates the document it grades.
#[cfg(feature = "openapi")]
#[path = "openapi_artifact.rs"]
mod openapi_artifact;
#[cfg(feature = "openapi")]
#[path = "openapi_codes.rs"]
mod openapi_codes;
#[cfg(feature = "openapi")]
#[path = "openapi_coverage.rs"]
mod openapi_coverage;
#[path = "route_inventory.rs"]
mod route_inventory;
#[path = "route_meta_total.rs"]
mod route_meta_total;
#[path = "route_verbs.rs"]
mod route_verbs;
#[path = "router.rs"]
mod router;
#[path = "span_route_template.rs"]
mod span_route_template;
