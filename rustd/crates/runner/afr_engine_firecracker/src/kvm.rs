//! The KVM device a microVM needs, and the boot admission that refuses a host
//! without it: no KVM, no Firecracker, and no other engine in its place.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use af_core::error_code::{Coded as _, Logged};
use af_wire::runner::Kvm;
use afr_engine::Result;

use crate::error::kvm_refused;

/// The device a microVM's monitor opens.
pub const KVM_PATH: &str = "/dev/kvm";
/// The event a host refused for its KVM device is logged under.
const EVENT_HOST_REFUSED: &str = "sandbox_host_refused";

/// What the device at `path` allows: opened for reading and writing, which is
/// what a monitor needs.
#[must_use]
pub fn probe(path: &Path) -> Kvm {
    match fs::OpenOptions::new().read(true).write(true).open(path) {
        Ok(_device) => Kvm::Usable,
        Err(error) if error.kind() == ErrorKind::NotFound => Kvm::Absent,
        Err(_denied) => Kvm::Denied,
    }
}

/// Nothing when `kvm` is usable; otherwise the refusal naming `kvm` and what
/// the probe read, logged.
///
/// # Errors
/// The device is denied or absent.
pub fn admit(kvm: Kvm) -> Result<()> {
    if kvm == Kvm::Usable {
        return Ok(());
    }
    let error = kvm_refused(kvm);
    let Logged { error_code, reason } = error.logged();
    let (missing, read) = (crate::MECHANISM_KVM, <&str>::from(kvm));
    let event = EVENT_HOST_REFUSED;
    tracing::error!(missing, kvm = read, error_code, reason, event);
    Err(error)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
    )]

    use af_wire::runner::Kvm;

    use super::{admit, probe};

    #[test]
    fn test_firecracker_refuses_a_host_without_usable_kvm() {
        let host = tempfile::tempdir().unwrap();

        assert_eq!(probe(&host.path().join("kvm")), Kvm::Absent);
        for kvm in [Kvm::Denied, Kvm::Absent] {
            let refused = admit(kvm).unwrap_err();
            assert_eq!(refused.missing_mechanism(), Some("kvm"));
            let reason = refused.detail();
            assert!(
                reason.contains(&format!("kvm is {}", <&str>::from(kvm))),
                "{reason}"
            );
        }
        admit(Kvm::Usable).unwrap();
    }
}
