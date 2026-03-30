const std = @import("std");

const log = std.log.scoped(.zombied);

pub fn run(alloc: std.mem.Allocator) !void {
    var args = std.process.args();
    _ = args.next(); // binary
    _ = args.next(); // "runs"
    const subcmd = args.next() orelse {
        std.debug.print("usage: zombied runs replay <run_id>\n", .{});
        std.process.exit(1);
    };
    if (!std.mem.eql(u8, subcmd, "replay")) {
        std.debug.print("unknown runs subcommand: {s}\n", .{subcmd});
        std.debug.print("usage: zombied runs replay <run_id>\n", .{});
        std.process.exit(1);
    }
    const run_id = args.next() orelse {
        std.debug.print("usage: zombied runs replay <run_id>\n", .{});
        std.process.exit(1);
    };

    const base_url = std.process.getEnvVarOwned(alloc, "ZOMBIED_API_URL") catch |err| {
        if (err == error.EnvironmentVariableNotFound) {
            std.debug.print("error: ZOMBIED_API_URL not set\n", .{});
            std.process.exit(1);
        }
        return err;
    };
    defer alloc.free(base_url);

    const api_key = std.process.getEnvVarOwned(alloc, "ZOMBIED_API_KEY") catch |err| {
        if (err == error.EnvironmentVariableNotFound) {
            std.debug.print("error: ZOMBIED_API_KEY not set\n", .{});
            std.process.exit(1);
        }
        return err;
    };
    defer alloc.free(api_key);

    const url = try std.fmt.allocPrint(alloc, "{s}/v1/runs/{s}:replay", .{ base_url, run_id });
    defer alloc.free(url);

    var client = std.http.Client{ .allocator = alloc };
    defer client.deinit();

    const auth_header = try std.fmt.allocPrint(alloc, "Bearer {s}", .{api_key});
    defer alloc.free(auth_header);

    var response_body: std.ArrayList(u8) = .{};
    defer response_body.deinit(alloc);
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &response_body);

    const uri = std.Uri.parse(url) catch {
        std.debug.print("error: invalid URL: {s}\n", .{url});
        std.process.exit(1);
    };

    const result = client.fetch(.{
        .method = .GET,
        .location = .{ .uri = uri },
        .extra_headers = &.{
            .{ .name = "Authorization", .value = auth_header },
            .{ .name = "Accept", .value = "application/json" },
        },
        .response_writer = &aw.writer,
    }) catch |err| {
        std.debug.print("error: HTTP request failed: {any}\n", .{err});
        std.process.exit(1);
    };

    if (result.status != .ok) {
        std.debug.print("error: server returned {d}\n{s}\n", .{ @intFromEnum(result.status), response_body.items });
        std.process.exit(1);
    }

    const parsed = std.json.parseFromSlice(std.json.Value, alloc, response_body.items, .{}) catch {
        std.debug.print("error: invalid JSON response\n", .{});
        std.process.exit(1);
    };
    defer parsed.deinit();

    renderReplay(parsed.value);
}

fn renderReplay(value: std.json.Value) void {
    const obj = switch (value) {
        .object => |o| o,
        else => {
            std.debug.print("error: unexpected response shape\n", .{});
            return;
        },
    };

    const run_id = if (obj.get("run_id")) |v| switch (v) {
        .string => |s| s,
        else => "?",
    } else "?";
    const state = if (obj.get("current_state")) |v| switch (v) {
        .string => |s| s,
        else => "?",
    } else "?";

    std.debug.print("\n=== Run Replay: {s} ===\n", .{run_id});
    std.debug.print("Final state: {s}\n\n", .{state});

    const gate_results = if (obj.get("gate_results")) |v| switch (v) {
        .array => |a| a.items,
        else => &[_]std.json.Value{},
    } else &[_]std.json.Value{};

    if (gate_results.len == 0) {
        std.debug.print("No gate results recorded.\n", .{});
        return;
    }

    var last_gate: []const u8 = "";
    for (gate_results) |entry| {
        const entry_obj = switch (entry) {
            .object => |o| o,
            else => continue,
        };

        const gate_name = if (entry_obj.get("gate_name")) |v| switch (v) {
            .string => |s| s,
            else => "?",
        } else "?";
        const attempt = if (entry_obj.get("attempt")) |v| switch (v) {
            .integer => |i| i,
            else => 0,
        } else 0;
        const exit_code = if (entry_obj.get("exit_code")) |v| switch (v) {
            .integer => |i| i,
            else => -1,
        } else -1;
        const wall_ms = if (entry_obj.get("wall_ms")) |v| switch (v) {
            .integer => |i| i,
            else => 0,
        } else 0;
        const stdout = if (entry_obj.get("stdout_tail")) |v| switch (v) {
            .string => |s| s,
            else => "",
        } else "";
        const stderr = if (entry_obj.get("stderr_tail")) |v| switch (v) {
            .string => |s| s,
            else => "",
        } else "";

        if (!std.mem.eql(u8, gate_name, last_gate)) {
            std.debug.print("--- Gate: {s} ---\n", .{gate_name});
            last_gate = gate_name;
        }

        const outcome = if (exit_code == 0) "PASS" else "FAIL";
        std.debug.print("  Loop {d}: {s} ({d}ms, exit={d})\n", .{ attempt, outcome, wall_ms, exit_code });

        if (stdout.len > 0) {
            std.debug.print("  stdout:\n{s}\n", .{stdout});
        }
        if (stderr.len > 0) {
            std.debug.print("  stderr:\n{s}\n", .{stderr});
        }
    }
    std.debug.print("\n", .{});
}
