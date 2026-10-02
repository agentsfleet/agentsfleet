//! The engine end to end on a fake host: no root, no bubblewrap, no kernel
//! features. The formatter and `mount` are `true`, the cgroup root a plain
//! directory, "bubblewrap" a script, and the executor is served in-process on
//! the socket the engine waits for — so every step of `prepare` and every
//! branch of the release runs where an unprivileged test can reach it.
//!
//! Nothing on the fake host is really mounted, so every unmount is refused:
//! which is how these tests prove a disk that will not unmount keeps its image.

mod prepare;
mod release;
mod support;
