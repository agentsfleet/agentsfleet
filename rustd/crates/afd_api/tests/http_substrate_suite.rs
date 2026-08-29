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
#[path = "route_inventory.rs"]
mod route_inventory;
#[path = "route_meta_total.rs"]
mod route_meta_total;
#[path = "router.rs"]
mod router;
#[path = "span_route_template.rs"]
mod span_route_template;
