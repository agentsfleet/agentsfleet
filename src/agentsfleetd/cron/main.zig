//! Public facade for hosted recurring schedules.
//!
//! QStash owns timekeeping. This package owns schedule intent, synchronous
//! provider mutation, and signed-fire admission; it never starts a cron loop.

pub const model = @import("model.zig");
pub const validate = @import("validate.zig");
pub const Credentials = @import("Credentials.zig");
pub const Store = @import("Store.zig");
pub const QStashClient = @import("QStashClient.zig");
pub const QStashVerifier = @import("QStashVerifier.zig");
pub const FireQueue = @import("FireQueue.zig");
pub const FireService = @import("FireService.zig");
pub const FireStore = @import("FireStore.zig");
pub const Service = @import("Service.zig");

test {
    _ = @import("constants.zig");
    _ = @import("model_test.zig");
    _ = @import("store_test.zig");
    _ = @import("store_concurrency_test.zig");
    _ = @import("validate_test.zig");
    _ = @import("qstash_client_test.zig");
    _ = @import("qstash_verifier_test.zig");
    _ = @import("fire_queue_integration_test.zig");
    _ = @import("service_test.zig");
}
