use afr_egress::RESPONSE_MAX_BYTES;
use afr_egress::testing::inbound;

#[test]
fn should_omit_a_truncated_body_without_hiding_its_incomplete_status() {
    let body = "x".repeat(RESPONSE_MAX_BYTES);
    let mut response = inbound(200, &body);
    response.truncated = true;
    response.content_type = Some("text/plain".to_owned());

    let output = super::render(&response);

    assert!(output.starts_with("Status: 200\nContent-Type: text/plain\n"));
    assert!(output.contains(&format!(
        "[Response truncated at {RESPONSE_MAX_BYTES} bytes]"
    )));
    assert!(output.contains("Incomplete response body omitted"));
    assert!(!output.contains(&body));
    assert!(output.len() < 1024);
}
