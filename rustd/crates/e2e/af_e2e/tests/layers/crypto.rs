//! Which crates may name a cryptographic implementation or the QUIC stack:
//! the workspace computes its cryptography through one home on one backend,
//! AWS-LC, and speaks QUIC through one crate. Each rule has a twin that plants
//! the violation and expects it named.

use crate::workspace::{Kind, Workspace, planted};

/// The cryptographic implementations: AWS-LC and the crates it replaced.
const CRYPTO: [&str; 8] = [
    "sha2",
    "sha1",
    "hmac",
    "aes-gcm",
    "subtle",
    "getrandom",
    "ring",
    "aws-lc-rs",
];

/// `af_crypto`, the home; `af_core`, which `af_crypto` depends on and so
/// cannot be served by it; and `guffbolt-ai`, which is published alone.
const CRYPTO_NAMERS: [&str; 3] = ["af_crypto", "af_core", "guffbolt-ai"];

/// `BoringSSL` and the QUIC stack over it, the runner channel's transport.
const QUIC: [&str; 3] = ["boring", "tokio-boring", "tokio-quiche"];

/// The one crate the runner channel's QUIC lives in.
const QUIC_NAMERS: [&str; 1] = ["af_stream"];

/// Every normal or build edge, optional ones included, naming a cryptographic
/// or QUIC crate from a member not allowed to. Dev tables are exempt, as the
/// datastore rule's are: a bench measuring another implementation links
/// nothing into what ships.
pub(crate) fn crypto_edges(workspace: &Workspace) -> Vec<String> {
    let mut violations = Vec::new();
    for member in workspace.members.values() {
        for dep in member.deps.iter().filter(|dep| dep.kind != Kind::Dev) {
            let named = dep.name.as_str();
            let allowed: &[&str] = if CRYPTO.contains(&named) {
                &CRYPTO_NAMERS
            } else if QUIC.contains(&named) {
                &QUIC_NAMERS
            } else {
                continue;
            };
            if !allowed.contains(&member.name.as_str()) {
                violations.push(format!(
                    "{} ({}) {:?} → {named}: only {} may name it",
                    member.name,
                    member.dir,
                    dep.kind,
                    allowed.join(", ")
                ));
            }
        }
    }
    violations
}

#[test]
fn only_the_crypto_home_names_a_cryptographic_crate_and_only_af_stream_names_quic() {
    let violations = crypto_edges(&Workspace::load());
    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn a_planted_cryptographic_or_quic_edge_is_named() {
    let workspace = planted(&[
        (
            "crates/daemon/afd_x",
            "afd_x",
            "[dependencies]\nsha2.workspace = true\n[target.'cfg(unix)'.dependencies]\nsubtle.workspace = true\n",
        ),
        (
            "crates/runner/afr_y",
            "afr_y",
            "[build-dependencies]\ntokio-quiche.workspace = true\n[dev-dependencies]\naws-lc-rs.workspace = true\n",
        ),
        (
            "crates/foundation/af_crypto",
            "af_crypto",
            "[dependencies]\naws-lc-rs.workspace = true\nboring.workspace = true\n",
        ),
        (
            "crates/protocol/af_stream",
            "af_stream",
            "[dependencies]\ntokio-quiche.workspace = true\n",
        ),
        (
            "guffbolt-ai",
            "guffbolt-ai",
            "[dependencies]\naws-lc-rs = { workspace = true, optional = true }\n",
        ),
    ]);
    assert_eq!(
        crypto_edges(&workspace),
        [
            "af_crypto (crates/foundation/af_crypto) Normal → boring: only af_stream may name it",
            "afd_x (crates/daemon/afd_x) Normal → sha2: only af_crypto, af_core, guffbolt-ai may name it",
            "afd_x (crates/daemon/afd_x) Normal → subtle: only af_crypto, af_core, guffbolt-ai may name it",
            "afr_y (crates/runner/afr_y) Build → tokio-quiche: only af_stream may name it",
        ]
    );
}
