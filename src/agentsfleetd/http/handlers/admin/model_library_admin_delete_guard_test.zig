const std = @import("std");
const model_library_store = @import("../../../state/model_library_store.zig");
const base = @import("model_library_admin_integration_test.zig");

const ALLOC = std.testing.allocator;

// The delete-guard's reference check must fail closed on a DB fault. Before the
// fix, isReferencedByActiveDefault swallowed a query error to `false`.
test "test_delete_blocked_on_referenced_check_query_error" {
    const h = try base.startHarness(ALLOC);
    defer h.deinit();
    defer base.cleanup(h);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    if (model_library_store.isReferencedByActiveDefault(conn, "not-a-uuid")) |_| {
        return error.QueryErrorSwallowedAsBool;
    } else |_| {}
}
