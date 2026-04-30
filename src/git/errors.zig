//! Shared error set for git submodules.
//!
//! Currently scoped to the runtime-artifact cleanup path. `command.run`
//! returns `CommandFailed` on non-zero exit or signal termination.

pub const GitError = error{
    CommandFailed,
};
