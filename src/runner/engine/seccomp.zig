//! seccomp.zig — syscall wall for the sandboxed child (after no_new_privs +
//! Landlock, before the engine). A BPF deny-filter traps dangerous syscalls;
//! the trap exits the child SECCOMP_VIOLATION_EXIT (a clean code — it survives
//! bwrap's signal->exit translation) which the parent maps to landlock_deny.
//! Enforcement holds even if a hostile agent installs its own SIGSYS handler:
//! RET_TRAP never executes the syscall. Numbers come from std.os.linux.SYS
//! (arch-correct). Linux-only; a no-op elsewhere.

const std = @import("std");
const builtin = @import("builtin");
const logging = @import("log");
const pipe_proto = @import("../pipe_proto.zig");

const log = logging.scoped(.runner_seccomp);

const SeccompError = error{ UnsupportedPlatform, FilterInstallFailed };

const SockFilter = extern struct { code: u16, jt: u8, jf: u8, k: u32 };
const SockFprog = extern struct { len: u16, filter: [*]const SockFilter };

const BPF_LD: u16 = 0x00;
const BPF_W: u16 = 0x00;
const BPF_ABS: u16 = 0x20;
const BPF_JMP: u16 = 0x05;
const BPF_JEQ: u16 = 0x10;
const BPF_K: u16 = 0x00;
const BPF_RET: u16 = 0x06;
const SECCOMP_RET_TRAP: u32 = 0x0003_0000;
const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
const PR_SET_SECCOMP: usize = 22;
const SECCOMP_MODE_FILTER: usize = 2;
const OFF_NR: u32 = 0;
const OFF_ARCH: u32 = 4;

// AUDIT_ARCH guards the foreign-ABI bypass; runner ships x86_64 + aarch64 only.
const AUDIT_ARCH: u32 = switch (builtin.cpu.arch) {
    .x86_64 => 0xC000_003E,
    .aarch64 => 0xC000_00B7,
    else => 0,
};

// Cross-process introspection, root/mount surgery, kernel module + kexec, bpf,
// perf, reboot, swap. exec/fork/file/socket stay allowed (bash tool + engine).
const DENIED = [_]std.os.linux.SYS{
    .ptrace,
    .process_vm_readv,
    .process_vm_writev,
    .mount,
    .umount2,
    .pivot_root,
    .kexec_load,
    .init_module,
    .finit_module,
    .delete_module,
    .bpf,
    .perf_event_open,
    .reboot,
    .swapon,
    .swapoff,
};

// load arch -> trap foreign ABI -> load nr -> trap each denied syscall -> allow.
fn buildProgram() [4 + DENIED.len * 2 + 1]SockFilter {
    var prog: [4 + DENIED.len * 2 + 1]SockFilter = undefined;
    prog[0] = .{ .code = BPF_LD | BPF_W | BPF_ABS, .jt = 0, .jf = 0, .k = OFF_ARCH };
    prog[1] = .{ .code = BPF_JMP | BPF_JEQ | BPF_K, .jt = 1, .jf = 0, .k = AUDIT_ARCH };
    prog[2] = .{ .code = BPF_RET | BPF_K, .jt = 0, .jf = 0, .k = SECCOMP_RET_TRAP };
    prog[3] = .{ .code = BPF_LD | BPF_W | BPF_ABS, .jt = 0, .jf = 0, .k = OFF_NR };
    for (DENIED, 0..) |sys, i| {
        prog[4 + i * 2] = .{ .code = BPF_JMP | BPF_JEQ | BPF_K, .jt = 0, .jf = 1, .k = @intCast(@intFromEnum(sys)) };
        prog[4 + i * 2 + 1] = .{ .code = BPF_RET | BPF_K, .jt = 0, .jf = 0, .k = SECCOMP_RET_TRAP };
    }
    prog[4 + DENIED.len * 2] = .{ .code = BPF_RET | BPF_K, .jt = 0, .jf = 0, .k = SECCOMP_RET_ALLOW };
    return prog;
}

const PROGRAM = buildProgram();

// Linux types the signo as std.os.linux.SIG, other platforms as i32.
const Signo = if (builtin.os.tag == .linux) std.os.linux.SIG else i32;

// Async-signal-safe: a lone exit_group(2), no libc/atexit/stdio.
fn onSigsys(_: Signo) callconv(.c) void {
    _ = std.os.linux.syscall1(.exit_group, @as(usize, pipe_proto.SECCOMP_VIOLATION_EXIT));
    unreachable;
}

/// Install the SIGSYS handler then the filter. Single-threaded here, so the
/// filter covers every thread the engine later spawns. Fail-closed: any failure
/// returns an error and the caller exits SANDBOX_FAIL_EXIT (Invariant 7).
pub fn applyFilter() SeccompError!void {
    if (builtin.os.tag != .linux) return SeccompError.UnsupportedPlatform;
    var act = std.posix.Sigaction{
        .handler = .{ .handler = onSigsys },
        .mask = std.mem.zeroes(std.posix.sigset_t),
        .flags = 0,
    };
    std.posix.sigaction(std.posix.SIG.SYS, &act, null);
    const prog = SockFprog{ .len = @intCast(PROGRAM.len), .filter = &PROGRAM };
    const rc = std.os.linux.prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, @intFromPtr(&prog), 0, 0);
    if (rc != 0) {
        log.err("seccomp_filter_install_failed", .{ .rc = rc });
        return SeccompError.FilterInstallFailed;
    }
    log.info("applied", .{ .denied = DENIED.len });
}

test "applyFilter returns UnsupportedPlatform off-linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    try std.testing.expectError(SeccompError.UnsupportedPlatform, applyFilter());
}

test "filter program is well-formed: arch guard, nr load, one trap per denied syscall, default allow" {
    try std.testing.expectEqual(@as(usize, 4 + DENIED.len * 2 + 1), PROGRAM.len);
    try std.testing.expectEqual(@as(u32, OFF_ARCH), PROGRAM[0].k);
    try std.testing.expectEqual(@as(u32, AUDIT_ARCH), PROGRAM[1].k);
    try std.testing.expectEqual(SECCOMP_RET_TRAP, PROGRAM[2].k);
    try std.testing.expectEqual(@as(u32, OFF_NR), PROGRAM[3].k);
    try std.testing.expectEqual(SECCOMP_RET_ALLOW, PROGRAM[PROGRAM.len - 1].k);
    for (DENIED, 0..) |sys, i| {
        try std.testing.expectEqual(@as(u32, @intCast(@intFromEnum(sys))), PROGRAM[4 + i * 2].k);
        try std.testing.expectEqual(SECCOMP_RET_TRAP, PROGRAM[4 + i * 2 + 1].k);
    }
}
