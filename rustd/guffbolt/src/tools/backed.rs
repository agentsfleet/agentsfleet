//! The shape every tool over one backend shares: it holds the backend it was
//! built with, borrowed for as long as the run that built it.

/// Declares `$tool`, which holds a `&'run dyn $backend` and is built with
/// `new(backend)`. Its `Handler` is written beside it.
macro_rules! backed {
    ($(#[$doc:meta])* $tool:ident over $backend:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy)]
        pub struct $tool<'run> {
            backend: &'run dyn $backend,
        }

        impl<'run> $tool<'run> {
            /// The tool, calling `backend`.
            #[must_use]
            pub const fn new(backend: &'run dyn $backend) -> Self {
                Self { backend }
            }
        }
    };
}
