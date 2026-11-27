//! What the agent proves before it touches anything: that it is a microVM's
//! first process, with a vsock device to serve the host on.
//!
//! It mounts over `/`, reaps every orphan and powers the machine off when it
//! ends, so anywhere else it would wreck the system it ran on. The check reads
//! the process and the kernel through [`Machine`], so a test states a machine
//! rather than needing one.

use crate::error::{Result, not_init};

/// The identifier the kernel gives the first process it starts.
const INIT_PID: u32 = 1;

/// What the agent reads of the process and machine it runs on.
pub trait Machine {
    /// This process's identifier.
    fn pid(&self) -> u32;

    /// Whether the kernel offers vsock: a socket of that family can be made.
    fn vsock(&self) -> bool;
}

/// The machine this process runs on.
#[derive(Debug, Clone, Copy, Default)]
pub struct Booted;

impl Machine for Booted {
    fn pid(&self) -> u32 {
        std::process::id()
    }

    #[cfg(target_os = "linux")]
    fn vsock(&self) -> bool {
        use rustix::net::{AddressFamily, SocketType, socket};

        socket(AddressFamily::VSOCK, SocketType::STREAM, None).is_ok()
    }

    /// Only a Linux guest kernel has the vsock device the host dials.
    #[cfg(not(target_os = "linux"))]
    fn vsock(&self) -> bool {
        false
    }
}

/// Refuses unless `machine` says this is the kernel's first process with a
/// vsock device, naming which it is not.
///
/// # Errors
/// The process is not process 1, or the kernel offers no vsock.
pub fn admit(machine: &impl Machine) -> Result<()> {
    let pid = machine.pid();
    if pid != INIT_PID {
        return Err(not_init(format!(
            "it is process {pid}, not process {INIT_PID}: it runs only as a microVM's /init"
        )));
    }
    if !machine.vsock() {
        return Err(not_init(
            "the kernel offers no vsock device, the host's only way in".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Machine, admit};

    /// A machine that answers what it was given.
    struct Stated {
        pid: u32,
        vsock: bool,
    }

    impl Machine for Stated {
        fn pid(&self) -> u32 {
            self.pid
        }

        fn vsock(&self) -> bool {
            self.vsock
        }
    }

    /// Run on a host, or as any process but the first, the agent refuses
    /// before it mounts anything, naming the process it is.
    #[test]
    fn test_the_agent_refuses_unless_it_is_process_one() {
        let refused = admit(&Stated {
            pid: 4242,
            vsock: true,
        })
        .err()
        .map(|refused| refused.told());

        assert!(
            refused
                .as_deref()
                .is_some_and(|told| told.contains("process 4242, not process 1")),
            "{refused:?}"
        );
    }

    /// The first process of a kernel with no vsock has no host to serve.
    #[test]
    fn test_the_agent_refuses_a_kernel_without_vsock() {
        let refused = admit(&Stated {
            pid: 1,
            vsock: false,
        })
        .err()
        .map(|refused| refused.told());

        assert!(
            refused
                .as_deref()
                .is_some_and(|told| told.contains("no vsock device")),
            "{refused:?}"
        );
    }

    /// The first process of a kernel with vsock is admitted.
    #[test]
    fn test_the_first_process_with_vsock_is_admitted() {
        let admitted = admit(&Stated {
            pid: 1,
            vsock: true,
        });

        assert!(admitted.err().is_none());
    }

    /// This test process is never process 1, so the real machine refuses it.
    #[test]
    fn test_the_real_machine_refuses_a_test_process() {
        assert!(admit(&super::Booted).is_err());
    }
}
