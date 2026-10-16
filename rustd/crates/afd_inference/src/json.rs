//! Move SDK JSON bytes into a physical request without copying headers or body.

use rig_core::{http_client::Request, wire::Body};

use crate::{Result, error::ErrorKind};

/// Preserves the encoded request while refusing upload-only multipart bodies.
///
/// # Errors
/// Multipart uploads cannot be used for configured text inference.
pub fn json_request(request: Request<Body>) -> Result<Request<Vec<u8>>> {
    let (parts, body) = request.into_parts();
    match body {
        Body::Bytes(bytes) => Ok(Request::from_parts(parts, bytes)),
        Body::Multipart(_) => Err(ErrorKind::Body.into()),
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "fixture construction and refusals must fail loudly"
)]
mod tests {
    use super::json_request;
    use rig_core::{
        http_client::{MultipartForm, Request},
        wire::Body,
    };

    #[test]
    fn json_conversion_preserves_the_complete_request_and_allocation() {
        let bytes = br#"{"prompt":"private prompt"}"#.to_vec();
        let allocation = bytes.as_ptr();
        let request = Request::builder()
            .method("POST")
            .uri("https://models.example/v1/custom")
            .header("x-private-key", "private credential")
            .body(Body::Bytes(bytes))
            .expect("request fixture");
        let converted = json_request(request).expect("JSON request");
        assert_eq!(converted.method(), "POST");
        assert_eq!(converted.uri(), "https://models.example/v1/custom");
        assert_eq!(converted.headers()["x-private-key"], "private credential");
        assert_eq!(converted.body(), br#"{"prompt":"private prompt"}"#);
        assert_eq!(converted.body().as_ptr(), allocation);
    }

    #[test]
    fn multipart_is_refused_without_disclosing_its_content() {
        let request = Request::new(Body::Multipart(
            MultipartForm::new().text("prompt", "private prompt"),
        ));
        let error = json_request(request).expect_err("multipart is not text inference");
        assert_eq!(error.code(), afd_core::error_code::INVALID_REQUEST);
        assert_eq!(
            error.to_string(),
            "[UZ-REQ-001] inference request must contain JSON bytes"
        );
        assert!(!format!("{error:?}").contains("private prompt"));
        let sdk = rig_core::ProviderError::from(error);
        assert!(!format!("{sdk:?}").contains("private prompt"));
        assert!(matches!(sdk, rig_core::ProviderError::Http(_)));
        if let rig_core::ProviderError::Http(transport) = sdk {
            let source = std::error::Error::source(transport.as_ref())
                .expect("typed inference cause behind SDK transport");
            assert!(source.is::<crate::Error>());
            assert!(
                source
                    .to_string()
                    .contains("inference request must contain JSON bytes")
            );
        }
    }
}
