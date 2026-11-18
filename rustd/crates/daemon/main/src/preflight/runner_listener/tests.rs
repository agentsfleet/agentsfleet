//! Invalid and incomplete listener settings are rejected together before any file is opened.

use af_core::env::MapEnv;

use super::{LISTEN_ADDR, TLS_CERT_FILE, TLS_KEY_FILE, read};

#[test]
fn missing_identity_names_both_files() {
    let mut faults = Vec::new();
    assert!(read(&MapEnv::from_pairs([]), &mut faults).is_none());
    assert_eq!(
        faults.iter().map(super::Fault::knob).collect::<Vec<_>>(),
        vec![TLS_CERT_FILE, TLS_KEY_FILE]
    );
}

#[test]
fn malformed_address_does_not_hide_missing_identity() {
    let mut faults = Vec::new();
    let env = MapEnv::from_pairs([(LISTEN_ADDR, "https://daemon.internal:4443")]);
    assert!(read(&env, &mut faults).is_none());
    assert_eq!(
        faults.iter().map(super::Fault::knob).collect::<Vec<_>>(),
        vec![LISTEN_ADDR, TLS_CERT_FILE, TLS_KEY_FILE]
    );
}

#[test]
fn address_defaults_to_the_private_native_port() {
    let mut faults = Vec::new();
    let env = MapEnv::from_pairs([
        (TLS_CERT_FILE, "/run/runner/chain.pem"),
        (TLS_KEY_FILE, "/run/runner/leaf.pem"),
    ]);
    let config = read(&env, &mut faults);
    assert!(faults.is_empty(), "unexpected faults: {faults:?}");
    assert_eq!(
        config.map(|value| value.address.to_string()).as_deref(),
        Some("[::]:4443")
    );
}

#[test]
fn port_zero_is_retained_for_a_bound_fixture_listener() {
    let mut faults = Vec::new();
    let env = MapEnv::from_pairs([
        (LISTEN_ADDR, "127.0.0.1:0"),
        (TLS_CERT_FILE, "/run/runner/chain.pem"),
        (TLS_KEY_FILE, "/run/runner/leaf.pem"),
    ]);
    let config = read(&env, &mut faults);
    assert!(faults.is_empty(), "unexpected faults: {faults:?}");
    assert_eq!(
        config.map(|value| value.address.to_string()).as_deref(),
        Some("127.0.0.1:0")
    );
}

#[test]
fn one_identity_file_does_not_enable_the_listener() {
    for (present, missing) in [(TLS_CERT_FILE, TLS_KEY_FILE), (TLS_KEY_FILE, TLS_CERT_FILE)] {
        let mut faults = Vec::new();
        let env = MapEnv::from_pairs([(present, "/run/runner/identity.pem")]);
        assert!(read(&env, &mut faults).is_none());
        assert_eq!(
            faults.iter().map(super::Fault::knob).collect::<Vec<_>>(),
            vec![missing]
        );
    }
}

#[test]
fn udp_payload_ceiling_is_validated_before_binding() {
    for value in ["1199", "65508", "invalid"] {
        let env = MapEnv::from_pairs([
            (TLS_CERT_FILE, "/run/runner/chain.pem"),
            (TLS_KEY_FILE, "/run/runner/leaf.pem"),
            (super::MAX_UDP_PAYLOAD_BYTES, value),
        ]);
        let mut faults = Vec::new();
        assert!(read(&env, &mut faults).is_none());
        assert_eq!(
            faults.iter().map(super::Fault::knob).collect::<Vec<_>>(),
            [super::MAX_UDP_PAYLOAD_BYTES]
        );
    }
    let env = MapEnv::from_pairs([
        (TLS_CERT_FILE, "/run/runner/chain.pem"),
        (TLS_KEY_FILE, "/run/runner/leaf.pem"),
        (super::MAX_UDP_PAYLOAD_BYTES, "1200"),
    ]);
    let mut faults = Vec::new();
    assert_eq!(
        read(&env, &mut faults).map(|config| config.udp_payload_limit.bytes()),
        Some(1200)
    );
    assert!(faults.is_empty(), "unexpected faults: {faults:?}");
}
