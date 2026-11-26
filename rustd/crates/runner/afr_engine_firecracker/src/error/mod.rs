//! What only the Firecracker engine refuses.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. The engine answers the seam's errors, into
//! which every building block's failure lifts; this type holds what no
//! building block raises: a host whose KVM device a microVM cannot open, a
//! workspace image another monitor holds, a monitor that exited or never
//! answered, a host whose every jail user is held. It crosses the seam as
//! itself, and a refused KVM device still reads as the seam's missing
//! mechanism. The code is the registry's existing one, as every runner
//! crate's is (`docs/RUST_ERROR_STANDARD.md`).

use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::time::Duration;

use af_core::error_code::{self, ErrorCode};
use af_wire::runner::Kvm;

/// The mechanism a host without a usable KVM device lacks, as the seam's
/// refusal and the capability report's check both name it.
pub const MECHANISM_KVM: &str = "kvm";

af_core::error_shell!(
    /// A Firecracker failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails on its own.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The KVM device is not one a microVM can open.
    #[error("this host cannot run a microVM: kvm is {}, {}", <&str>::from(*.kvm), reason(*.kvm))]
    Kvm {
        /// What the probe read.
        kvm: Kvm,
        /// The seam's refusal it carries, naming the mechanism.
        seam: afr_engine::Error,
    },

    /// Another monitor holds the workspace image's lock.
    #[error("the workspace image {} is locked by a running microVM", .image.display())]
    ImageHeld {
        /// The image.
        image: PathBuf,
    },

    /// Every jail user this host hands out is held.
    #[cfg(target_os = "linux")]
    #[error("every one of the {held} jail users of this host is held by a microVM")]
    JailsHeld {
        /// How many there are.
        held: u32,
    },

    /// The monitor exited before its guest answered.
    #[cfg(target_os = "linux")]
    #[error("the microVM's monitor exited before its guest answered: {status}")]
    Exited {
        /// How it ended.
        status: std::process::ExitStatus,
    },

    /// The guest's executor did not answer within the ready timeout.
    #[cfg(target_os = "linux")]
    #[error("the guest's executor did not answer within {waited:?}")]
    NotReady {
        /// How long the engine waited.
        waited: Duration,
        /// Why the last attempt failed.
        #[source]
        source: afr_executor::Error,
    },

    /// The monitor's configuration would not render.
    #[cfg(target_os = "linux")]
    #[error("the microVM's configuration would not render")]
    Render {
        /// The encoder's reason.
        #[source]
        source: serde_json::Error,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under: every kind's is the
    /// registry's internal-operation code, as every runner crate's is.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        error_code::INTERNAL_OPERATION_FAILED
    }

    /// The image whose lock refused an attach, when that is the failure.
    #[must_use]
    pub fn held_image(&self) -> Option<&std::path::Path> {
        if let ErrorKind::ImageHeld { image } = self.kind() {
            Some(image)
        } else {
            None
        }
    }
}

impl afr_engine::Failure for Error {
    /// The missing mechanism a refused KVM device is; none otherwise.
    fn seam(&self) -> Option<&afr_engine::Error> {
        if let ErrorKind::Kvm { seam, .. } = self.kind() {
            Some(seam)
        } else {
            None
        }
    }
}

impl From<Error> for afr_engine::Error {
    fn from(error: Error) -> Self {
        Self::engine(error)
    }
}

/// What the probe's reading means for a microVM.
const fn reason(kvm: Kvm) -> &'static str {
    match kvm {
        Kvm::Usable => "and opens",
        Kvm::Denied => "present and opening it was refused",
        Kvm::Absent => "missing: the host has no /dev/kvm",
    }
}

/// Refuses a host whose KVM device reads `kvm`.
pub(crate) fn kvm_refused(kvm: Kvm) -> afr_engine::Error {
    Error::from(ErrorKind::Kvm {
        kvm,
        seam: afr_engine::error::refused(MECHANISM_KVM),
    })
    .into()
}

/// Refuses a second attach of `image`.
pub(crate) fn image_held(image: PathBuf) -> afr_engine::Error {
    Error::from(ErrorKind::ImageHeld { image }).into()
}

/// Refuses a microVM when every jail user is held.
#[cfg(target_os = "linux")]
pub(crate) fn jails_held(held: u32) -> afr_engine::Error {
    Error::from(ErrorKind::JailsHeld { held }).into()
}

/// Reports a monitor that exited before its guest answered.
#[cfg(target_os = "linux")]
pub(crate) fn exited(status: std::process::ExitStatus) -> afr_engine::Error {
    Error::from(ErrorKind::Exited { status }).into()
}

/// Reports a guest whose executor was not reached within `waited`, with why.
#[cfg(target_os = "linux")]
pub(crate) fn not_ready(waited: Duration) -> impl Fn(afr_executor::Error) -> afr_engine::Error {
    move |source| Error::from(ErrorKind::NotReady { waited, source }).into()
}

/// Reports a configuration that would not render.
#[cfg(target_os = "linux")]
pub(crate) fn unrendered(source: serde_json::Error) -> afr_engine::Error {
    Error::from(ErrorKind::Render { source }).into()
}
