use afd_crypto::secret::SecretBytes;

use super::{Relay, SMTPS_PORT, Security};

const SUBMISSION_PORT: u16 = 587;
const MAILPIT_PORT: u16 = 1025;

fn bag(json: &str) -> SecretBytes {
    SecretBytes::new(json.as_bytes().to_vec())
}

/// Dimension 2.7: no host on the network is ever reached in plaintext, and
/// loopback is.
#[test]
fn test_plaintext_refused_off_loopback() {
    for port in [SMTPS_PORT, SUBMISSION_PORT, MAILPIT_PORT] {
        for host in [
            "smtp.resend.com",
            "smtp.example.test",
            "10.0.0.5",
            "localhost.example.test",
        ] {
            assert_ne!(Security::of(host, port), Security::Plain, "{host}:{port}");
        }
    }
    for host in ["127.0.0.1", "127.0.0.9", "::1", "localhost", "LOCALHOST"] {
        assert_eq!(Security::of(host, MAILPIT_PORT), Security::Plain, "{host}");
    }
}

/// Port 465 is implicit TLS; every other remote port must upgrade.
#[test]
fn remote_ports_pick_their_tls() {
    assert_eq!(
        Security::of("smtp.resend.com", SMTPS_PORT),
        Security::Implicit
    );
    assert_eq!(
        Security::of("smtp.resend.com", SUBMISSION_PORT),
        Security::StartTls
    );
}

/// A complete bag parses; every incomplete or malformed one reads as absent.
#[test]
fn only_a_complete_bag_is_a_relay() {
    let complete = r#"{"host":"127.0.0.1","port":"1025","username":"u","password":"p","from_address":"hello@agentsfleet.test"}"#;
    assert!(Relay::parse(&bag(complete)).is_some());
    let unusable = [
        "not json",
        "{}",
        r#"{"host":"h","port":"1025","username":"u","password":"p"}"#,
        r#"{"host":"h","port":"x","username":"u","password":"p","from_address":"a@b.test"}"#,
        r#"{"host":"h","port":"1025","username":"u","password":"","from_address":"a@b.test"}"#,
        r#"{"host":"h","port":"1025","username":"u","password":"p","from_address":"not an address"}"#,
        r#"{"host":"h","port":1025,"username":"u","password":"p","from_address":"a@b.test"}"#,
    ];
    for json in unusable {
        assert!(Relay::parse(&bag(json)).is_none(), "{json}");
    }
}
