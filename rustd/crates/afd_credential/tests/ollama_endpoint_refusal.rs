//! No Ollama override is authorized before customer-owned placement is designed.

use afd_credential::provider::{Rejection, validate_endpoint_pair};

#[test]
fn ollama_cannot_bypass_endpoint_guards_with_a_local_or_cloud_override() {
    for endpoint in [
        "http://localhost:11434",
        "http://192.168.1.10:11434",
        "http://models.example.com",
        "https://127.0.0.1",
        "https://models.example.com",
    ] {
        assert_eq!(
            validate_endpoint_pair("ollama", Some(endpoint)),
            Err(Rejection::NotPermitted),
            "{endpoint}"
        );
    }
}

#[test]
fn the_named_ollama_cloud_route_needs_no_custom_endpoint_permission() {
    assert_eq!(validate_endpoint_pair("ollama", None), Ok(()));
}
