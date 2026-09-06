//! Scheduler credentials must not escape through diagnostic formatting.
use afd_cron::{SigningKeys, qstash::QStash};

#[test]
fn signing_keys_do_not_render_either_rotation_key() {
    let keys = SigningKeys {
        current: afd_crypto::secret::SecretString::new("current-private-value".to_owned()),
        next: afd_crypto::secret::SecretString::new("next-private-value".to_owned()),
    };
    let rendered = format!("{keys:?}");
    assert!(!rendered.contains("current-private-value"));
    assert!(!rendered.contains("next-private-value"));
}

#[test]
fn scheduler_client_does_not_render_its_bearer() {
    let client = QStash::new(
        reqwest::Client::new(),
        afd_crypto::secret::SecretString::new("scheduler-private-value".to_owned()),
        "https://daemon.example.test/fires".to_owned(),
        "https://scheduler.example.test".to_owned(),
    );
    let rendered = format!("{client:?}");
    assert!(!rendered.contains("scheduler-private-value"));
    assert!(rendered.contains("scheduler.example.test"));
}
