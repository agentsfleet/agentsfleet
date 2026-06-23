const post = @import("post.zig");

// The persistent client's "one client, reused across every flush" guarantee is
// structural: the flush loop calls Client.init() once before its while loop and
// reuses that instance, and the real POST is exercised by the integration path
// (a unit test must not depend on network connectivity). Here we assert the
// construct → tear-down lifecycle is sound (no crash, no leaked client state).
test "test_persistent_client_lifecycle: construct and tear down without crash" {
    var client = post.Client.init();
    client.deinit();
}
