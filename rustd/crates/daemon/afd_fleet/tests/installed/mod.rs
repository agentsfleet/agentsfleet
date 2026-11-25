//! An assignment installs the fleet row as it stands: its name, model and
//! network, with no document between them.

#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

mod assignment;
