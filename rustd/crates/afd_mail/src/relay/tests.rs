#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::time::Duration;

use afd_crypto::secret::SecretBytes;

use super::{BagFault, Relay, SMTPS_PORT, Security, TlsCache};
use crate::test_util::{FROM, PASSWORD, bag_json};

const SUBMISSION_PORT: u16 = 587;
const MAILPIT_PORT: u16 = 1025;
/// A remote port that is neither 465 nor 587, so a transport carrying it
/// proves the bag's port overrode lettre's default.
const ALTERNATE_SUBMISSION_PORT: u16 = 2525;
const REMOTE_HOST: &str = "smtp.resend.com";
const OTHER_REMOTE_HOST: &str = "smtp.example.test";
const LOOPBACK_ADDRESS: &str = "127.0.0.1";
const NETWORK_ADDRESS: &str = "10.0.0.5";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

fn bag(json: &str) -> SecretBytes {
    SecretBytes::new(json.as_bytes().to_vec())
}

/// The relay a complete bag naming `host:port` parses to.
fn relay_at(host: &str, port: u16) -> Relay {
    Relay::parse(&bag(&bag_json(host, port))).expect("a complete bag is a relay")
}

/// The transport built for `host:port`, as lettre's `Debug` prints it: a built
/// transport exposes its settings no other way.
fn transport_shape(host: &str, port: u16) -> String {
    let transport = relay_at(host, port)
        .server
        .transport(COMMAND_TIMEOUT, &TlsCache::default())
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
            OTHER_REMOTE_HOST,
            NETWORK_ADDRESS,
            "localhost.example.test",
        ] {
            assert_ne!(Security::of(host, port), Security::Plain, "{host}:{port}");
        }
    }
    for host in [
        LOOPBACK_ADDRESS,
        "127.0.0.9",
        "::1",
        "localhost",
        "LOCALHOST",
    ] {
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

/// A complete bag parses to the server and sender it names, and its `Debug`
/// never prints the password.
#[test]
fn should_parse_complete_bag_without_printing_password() {
    let relay = relay_at(REMOTE_HOST, SUBMISSION_PORT);
    assert_eq!(relay.from.email.to_string(), FROM);
    let shape = format!("{relay:?}");
    assert!(shape.contains(REMOTE_HOST), "{shape}");
    assert!(!shape.contains(PASSWORD), "{shape}");
}

/// Every unusable bag is refused with the fault that names what to fix: a bag
/// that is not an object of strings as malformed, an absent, empty or `null`
/// field as missing by name, and a port or sender that will not parse as
/// unparsed by name.
#[test]
fn should_name_the_field_at_fault_when_bag_unusable() {
    let cases = [
        ("not json", BagFault::Malformed),
        ("[]", BagFault::Malformed),
        (
            r#"{"host":"h","port":1025,"username":"u","password":"p","from_address":"a@b.test"}"#,
            BagFault::Malformed,
        ),
        ("{}", BagFault::Missing("host")),
        (
            r#"{"host":"","port":"1025","username":"u","password":"p","from_address":"a@b.test"}"#,
            BagFault::Missing("host"),
        ),
        (
            r#"{"host":"h","port":"","username":"u","password":"p","from_address":"a@b.test"}"#,
            BagFault::Missing("port"),
        ),
        (
            r#"{"host":"h","port":"1025","password":"p","from_address":"a@b.test"}"#,
            BagFault::Missing("username"),
        ),
        (
            r#"{"host":"h","port":"1025","username":"u","password":"","from_address":"a@b.test"}"#,
            BagFault::Missing("password"),
        ),
        (
            r#"{"host":"h","port":"1025","username":"u","password":null,"from_address":"a@b.test"}"#,
            BagFault::Missing("password"),
        ),
        (
            r#"{"host":"h","port":"1025","username":"u","password":"p"}"#,
            BagFault::Missing("from_address"),
        ),
        (
            r#"{"host":"h","port":"x","username":"u","password":"p","from_address":"a@b.test"}"#,
            BagFault::Unparsed("port"),
        ),
        (
            r#"{"host":"h","port":"70000","username":"u","password":"p","from_address":"a@b.test"}"#,
            BagFault::Unparsed("port"),
        ),
        (
            r#"{"host":"h","port":"1025","username":"u","password":"p","from_address":"not an address"}"#,
            BagFault::Unparsed("from_address"),
        ),
    ];
    for (json, fault) in cases {
        assert_eq!(Relay::parse(&bag(json)).err(), Some(fault), "{json}");
    }
}

/// What the `unconfigured` record says for each fault: its name, and the
/// field's name where one is at fault — never a value from the bag.
#[test]
fn should_log_fault_by_name_and_field() {
    for (fault, name, field) in [
        (BagFault::Absent, "absent", None),
        (BagFault::Malformed, "malformed", None),
        (BagFault::Missing("password"), "missing", Some("password")),
        (BagFault::Unparsed("port"), "unparsed", Some("port")),
    ] {
        assert_eq!((fault.name(), fault.field()), (name, field), "{fault:?}");
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
    assert!(transport_shape(LOOPBACK_ADDRESS, MAILPIT_PORT).contains("tls: None"));
    assert!(transport_shape(NETWORK_ADDRESS, MAILPIT_PORT).contains("tls: Required"));
}

/// The TLS parameters are built for the relay's host once and kept, shared by
/// every clone of the mailer that holds the cache; a bag naming another host
/// still gets a transport, built for that host, while the first stays cached.
/// A loopback relay builds none. Whichever host is asked, cached or not, the
/// parameters handed back name that host, so a certificate is never checked
/// against another relay's name.
#[test]
fn should_build_tls_once_per_host() {
    let cache = TlsCache::default();
    let shared = cache.clone();
    assert_eq!(format!("{shared:?}"), "TlsCache(None)");

    let plain = relay_at(LOOPBACK_ADDRESS, MAILPIT_PORT).server;
    plain
        .transport(COMMAND_TIMEOUT, &cache)
        .expect("a loopback transport builds");
    assert_eq!(format!("{shared:?}"), "TlsCache(None)");

    for host in [REMOTE_HOST, REMOTE_HOST, OTHER_REMOTE_HOST] {
        relay_at(host, SMTPS_PORT)
            .server
            .transport(COMMAND_TIMEOUT, &cache)
            .expect("a TLS transport builds without dialling");
        assert_eq!(
            format!("{shared:?}"),
            format!("TlsCache(Some({REMOTE_HOST:?}))"),
            "{host}"
        );
    }
    for host in [
        REMOTE_HOST,
        OTHER_REMOTE_HOST,
        REMOTE_HOST,
        OTHER_REMOTE_HOST,
    ] {
        let parameters = cache
            .parameters(host)
            .expect("parameters build without dialling");
        assert_eq!(parameters.domain(), host, "built for the host asked");
    }
}
