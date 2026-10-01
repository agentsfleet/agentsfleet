#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::time::Duration;

use afd_crypto::secret::SecretBytes;

use super::{Relay, SMTPS_PORT, Security};

const SUBMISSION_PORT: u16 = 587;
const MAILPIT_PORT: u16 = 1025;
/// A remote port that is neither 465 nor 587, so a transport carrying it
/// proves the bag's port overrode lettre's default.
const ALTERNATE_SUBMISSION_PORT: u16 = 2525;
const REMOTE_HOST: &str = "smtp.resend.com";
const PASSWORD: &str = "relay-password";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

fn bag(json: &str) -> SecretBytes {
    SecretBytes::new(json.as_bytes().to_vec())
}

/// The relay a complete bag naming `host:port` parses to.
fn relay_at(host: &str, port: u16) -> Relay {
    let json = serde_json::json!({
        "host": host,
        "port": port.to_string(),
        "username": "relay",
        "password": PASSWORD,
        "from_address": "hello@agentsfleet.test",
    })
    .to_string();
    Relay::parse(&bag(&json)).expect("a complete bag is a relay")
}

/// The transport built for `host:port`, as lettre's `Debug` prints it: a built
/// transport exposes its settings no other way.
fn transport_shape(host: &str, port: u16) -> String {
    let transport = relay_at(host, port)
        .transport(COMMAND_TIMEOUT)
        .expect("a transport builds without dialling");
    format!("{transport:?}")
}

/// Dimension 2.7: no host on the network is ever reached in plaintext, and
/// loopback is.
#[test]
fn test_plaintext_refused_off_loopback() {
    for port in [SMTPS_PORT, SUBMISSION_PORT, MAILPIT_PORT] {
        for host in [
            REMOTE_HOST,
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
    assert_eq!(Security::of(REMOTE_HOST, SMTPS_PORT), Security::Implicit);
    assert_eq!(
        Security::of(REMOTE_HOST, SUBMISSION_PORT),
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

/// Both TLS arms build on the workspace's aws-lc-rs provider and the platform
/// verifier without dialling, so a provider or trust store this build cannot
/// construct fails here rather than as `tls_setup` on a live invite. Each
/// transport carries the bag's port, the TLS mode that port earns, the
/// per-command timeout, and a credential its `Debug` never prints. The same
/// port on a loopback host builds with no TLS, and on a remote one with it.
#[test]
fn transport_builds_for_implicit_and_starttls() {
    for (port, mode) in [
        (SMTPS_PORT, "tls: Wrapper"),
        (SUBMISSION_PORT, "tls: Required"),
        (ALTERNATE_SUBMISSION_PORT, "tls: Required"),
    ] {
        let shape = transport_shape(REMOTE_HOST, port);
        assert!(shape.contains(&format!("port: {port},")), "{shape}");
        assert!(shape.contains(mode), "{shape}");
        assert!(
            shape.contains(&format!("timeout: Some({COMMAND_TIMEOUT:?})")),
            "{shape}"
        );
        assert!(!shape.contains(PASSWORD), "{shape}");
    }
    assert!(transport_shape("127.0.0.1", MAILPIT_PORT).contains("tls: None"));
    assert!(transport_shape("10.0.0.5", MAILPIT_PORT).contains("tls: Required"));
}
