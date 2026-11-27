//! What a process the guest agent starts puts on itself, behind one seam.
//!
//! ```text
//!   prepare ─► before the fork, where allocating is allowed: the Landlock
//!              ruleset built, the seccomp programs already compiled
//!   apply   ─► in the process itself: no_new_privs ─► Landlock ─► seccomp
//!              ─► no capability survived
//! ```
//!
//! The guest agent chooses the confinement, never a tenant: no request names
//! one. [`Hardened`] is the only one, and it is what every tenant process
//! and [`crate::harden`] apply.

use std::fmt;
use std::io;
use std::sync::Arc;

use seccompiler::BpfProgram;

use super::linux::{HIGH_NUMBERS, file_system_rules, programs};
use crate::error::Result;

/// What one process runs on itself to be confined, once: system calls on
/// what [`Confinement::prepare`] built, nothing allocated, so it is sound
/// between fork and exec.
pub type Applying = Box<dyn FnMut() -> Result<(), Refused> + Send + Sync>;

/// The confinement a process the guest agent starts puts on itself before it
/// runs anything a tenant gave it.
///
/// A second implementation is for a process the guest agent starts that needs
/// namespaces of its own.
pub trait Confinement: fmt::Debug + Send + Sync {
    /// Builds what one process applies to itself. Called before that
    /// process is forked, where allocating is allowed.
    ///
    /// # Errors
    /// The kernel would not build a piece of it.
    fn prepare(&self) -> Result<Applying>;
}

/// The step of a confinement the kernel refused, as the refusing call
/// answered it: nothing is allocated to carry it, so a process between fork
/// and exec may return it.
#[derive(Debug)]
pub enum Refused {
    /// `no_new_privs`, or reading the capability sets back.
    System(rustix::io::Errno),
    /// Enforcing the Landlock ruleset.
    Landlock(landlock::RulesetError),
    /// Installing a seccomp program.
    Seccomp(seccompiler::Error),
    /// A capability is still effective or permitted after every step.
    Capability,
    /// It was applied already: one is prepared for each process.
    Spent,
}

impl From<Refused> for io::Error {
    /// The kernel's own answer where a system call gave one, and a denied
    /// permission otherwise: made without allocating.
    fn from(refused: Refused) -> Self {
        match refused {
            Refused::System(errno) => errno.into(),
            Refused::Landlock(_) | Refused::Seccomp(_) | Refused::Capability | Refused::Spent => {
                Self::from(io::ErrorKind::PermissionDenied)
            }
        }
    }
}

/// A tenant process's confinement: it gains no privilege, reads everywhere
/// and writes only beneath [`crate::WRITABLE`] and to
/// [`crate::WRITABLE_DEVICES`], makes and joins no namespace, and holds no
/// capability.
#[derive(Debug)]
pub struct Hardened {
    /// The seccomp programs every process installs, compiled once.
    seccomp: Arc<[BpfProgram; 2]>,
}

impl Hardened {
    /// The confinement, its seccomp programs compiled for this machine.
    ///
    /// # Errors
    /// A program would not compile.
    pub fn new() -> Result<Self> {
        Ok(Self {
            seccomp: Arc::new(programs()?),
        })
    }
}

impl Confinement for Hardened {
    fn prepare(&self) -> Result<Applying> {
        // Enforced once; built for this process alone.
        let mut rules = Some(file_system_rules()?);
        let seccomp = Arc::clone(&self.seccomp);
        Ok(Box::new(move || {
            rustix::thread::set_no_new_privs(true).map_err(Refused::System)?;
            let rules = rules.take().ok_or(Refused::Spent)?;
            rules.restrict_self().map_err(Refused::Landlock)?;
            for program in seccomp.iter() {
                seccompiler::apply_filter(program).map_err(Refused::Seccomp)?;
            }
            seccompiler::apply_filter(&HIGH_NUMBERS).map_err(Refused::Seccomp)?;
            let held = rustix::thread::capabilities(None).map_err(Refused::System)?;
            if held.effective.is_empty() && held.permitted.is_empty() {
                Ok(())
            } else {
                Err(Refused::Capability)
            }
        }))
    }
}
